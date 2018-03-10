/*!
 * This is some kind of a library for dataflow computation. It's still very experimental and may
 * become something completely different in the end.
 *
 * The end goal is to use it for procedural and generative art. It's inspired by Pure Data and
 * Max/MSP, but will probably have less focus on graphical programming. Modular live coding,
 * perhaps?
 *
 * This is iteration #3.
 */

use futures::prelude::*;
use futures::task::Context;

use crossbeam::sync::{AtomicOption, SegQueue};

use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::collections::{HashMap, VecDeque};
use std::mem;
use std::slice;
use std::any::TypeId;
use std::marker::PhantomData;

/// A lightweight persistent identifier for a node.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub usize);

/// A lightweight persistent identifier for a port. Only gauranteeed to be unique within a specific
/// node.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PortId(pub usize);

/// A graph holds a collection of Nodes. Nodes have a collection of Ports. Ports can be connected
/// to each other one-to-one.
pub struct Graph {
    nodes: RwLock<HashMap<NodeId, Arc<Node>>>,
    id_counter: AtomicUsize,
}

impl Graph {
    /// Make a new empty graph.
    pub fn new() -> Arc<Graph> {
        Arc::new(Graph {
            nodes: RwLock::new(HashMap::new()),
            id_counter: 0.into(),
        })
    }
    /// Construct a new node from the given metadata and argument.
    pub fn add_node(self: &Arc<Graph>) -> Arc<Interface> {
        let ifc = Arc::new(Interface::new(self));
        let node = Arc::new(Node {
            ifc: ifc.clone(),
        });
        self.nodes.write().unwrap().insert(node.id(), node);
        ifc
    }
    /// Delete a node by id.
    pub fn remove_node(&self, node: NodeId) -> Result<Arc<Node>, Error> {
        self.nodes
            .write()
            .unwrap()
            .remove(&node)
            .ok_or(Error::InvalidNode)
    }
    /// Returns a vector containing references to all nodes active at the time of the call.
    pub fn nodes(&self) -> Vec<Arc<Node>> {
        self.nodes.read().unwrap().values().cloned().collect()
    }
    /// Returns a hash map from id to node references for all nodes active at the time of the call.
    pub fn node_map(&self) -> HashMap<NodeId, Arc<Node>> {
        self.nodes.read().unwrap().clone()
    }
    /// Get a node by id.
    pub fn node(&self, id: NodeId) -> Option<Arc<Node>> {
        self.nodes.read().unwrap().get(&id).cloned()
    }

    fn generate_id(&self) -> usize {
        self.id_counter.fetch_add(1, Ordering::SeqCst)
    }
}

/// A node is the public interface for generic functionality on a module in the graph.
/// It holds a `Module`.
pub struct Node {
    ifc: Arc<Interface>,
}

impl Node {
    /// Get the node ID.
    pub fn id(&self) -> NodeId {
        self.ifc.id()
    }
    /// Find a port by name (name is held within the associated `MetaPort`)
    pub fn find_port(&self, name: &'static str) -> Option<Arc<OpaquePort>> {
        self.ifc.find_port(name)
    }
    /// Get a vector of references to all associated ports at the time of the call.
    pub fn ports(&self) -> Vec<Arc<OpaquePort>> {
        self.ifc.ports()
    }
}

/// The private interface for a module. The module is provided with an `Interface` upon construction.
/// An `Interface` has a superset of the functionality of a `Node`. It can be used to manipulate the
/// associated Ports.
pub struct Interface {
    id: NodeId,
    ports: RwLock<HashMap<PortId, Arc<OpaquePort>>>,
    graph: Weak<Graph>,
}

impl Interface {
    fn new(graph: &Arc<Graph>) -> Interface {
        Interface {
            id: NodeId(graph.generate_id()),
            ports: RwLock::new(HashMap::new()),
            graph: Arc::downgrade(graph),
        }
    }
    /// Get the node ID.
    pub fn id(&self) -> NodeId {
        self.id
    }
    /// Find a port by name (name is held within the associated `MetaPort`)
    pub fn find_port(&self, name: &str) -> Option<Arc<OpaquePort>> {
        self.ports
            .read()
            .unwrap()
            .iter()
            .find(|&(_, port)| port.name() == name)
            .map(|port| port.1)
            .cloned()
    }
    /// Get a vector of references to all associated ports at the time of the call.
    pub fn ports(&self) -> Vec<Arc<OpaquePort>> {
        self.ports.read().unwrap().values().cloned().collect()
    }
    /// Add a new port using the given metadata.
    pub fn add_port<I: 'static, O: 'static>(&self, name: String) -> Arc<Port<I, O>> {
        let port = Port::new(&self.graph.upgrade().unwrap(), name);
        self.ports
            .write()
            .unwrap()
            .insert(port.id, Arc::clone(port.as_opaque()));
        port
    }
    /// Remove a port by ID.
    pub fn remove_port(&self, port: PortId) -> Result<Arc<OpaquePort>, Error> {
        self.ports
            .write()
            .unwrap()
            .remove(&port)
            .ok_or(Error::InvalidPort)
    }
}

/// Ports are the connection points of modules. They can be connected one-to-one with other ports,
/// allowing data of type `I` to flow in and data of type `O` to flow out.
///
/// TODO think about interactions/problems with multiple graphs
pub struct Port<I: 'static, O: 'static> {
    _in: PhantomData<I>,
    _out: PhantomData<O>,
    in_ty: TypeId,
    out_ty: TypeId,
    name: String,
    id: PortId,
    buf_lock: AtomicBool,
    buf_lock_q: SegQueue<task::Waker>,
    buffer: UnsafeCell<VecDeque<u8>>,
    reader_buf: AtomicOption<task::Waker>,
    connect_wait: UnsafeCell<Vec<task::Waker>>,
    edge: Mutex<Option<Weak<Port<O, I>>>>,
    disconnect_occured: AtomicBool,
}

unsafe impl<I: 'static, O: 'static> Send for Port<I, O> {}
unsafe impl<I: 'static, O: 'static> Sync for Port<I, O> {}

/// An OpaquePort is a port with erased types at the type level. It can be downcast to a typed port
/// by calling `as_typed`.
pub type OpaquePort = Port<!, !>;

impl OpaquePort {
    /// Downcasts this `OpaquePort` to a port with the given types. Returns None if the given types
    /// do not match the underlying port.
    pub fn as_typed<'a, NewI: 'static, NewO: 'static>(
        self: &'a Arc<OpaquePort>,
    ) -> Option<&'a Arc<Port<NewI, NewO>>> {
        if TypeId::of::<NewI>() == self.in_ty && TypeId::of::<NewO>() == self.out_ty {
            Some(unsafe { mem::transmute::<&Arc<OpaquePort>, &Arc<Port<NewI, NewO>>>(self) })
        } else {
            None
        }
    }
}

impl<I: 'static, O: 'static> Port<I, O> {
    fn new(graph: &Graph, name: String) -> Arc<Port<I, O>> {
        assert!(mem::size_of::<I>() != 0);
        assert!(mem::size_of::<O>() != 0);
        Arc::new(Port {
            _in: PhantomData,
            _out: PhantomData,
            in_ty: TypeId::of::<I>(),
            out_ty: TypeId::of::<O>(),
            name,
            id: PortId(graph.generate_id()),
            buf_lock: AtomicBool::new(false),
            buf_lock_q: SegQueue::new(),
            buffer: UnsafeCell::new(VecDeque::new()),
            reader_buf: AtomicOption::new(),
            connect_wait: UnsafeCell::new(Vec::new()),
            edge: Mutex::new(None),
            disconnect_occured: AtomicBool::new(false),
        })
    }

    /// Erases types from the signature of this port, returning the corresponding OpaquePort.
    pub fn as_opaque<'a>(self: &'a Arc<Port<I, O>>) -> &'a Arc<OpaquePort> {
        unsafe { mem::transmute::<&Arc<Port<I, O>>, &Arc<OpaquePort>>(self) }
    }
    /// Get the PortId.
    pub fn id(&self) -> PortId {
        self.id
    }
    /// Get the port name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Connect this port to another. If either port is opaque and the ports have unmatched
    /// underlying types, this fails with ConnectError::TypeMismatch. Fails with
    /// ConnectError::AlreadyConnected if either port is already connected.
    pub fn connect(self: &Arc<Port<I, O>>, other: &Arc<Port<O, I>>) -> Result<(), ConnectError> {
        if self.in_ty != other.out_ty || self.out_ty != other.in_ty {
            return Err(ConnectError::TypeMismatch);
        }
        if self.id() == other.id() {
            // self edges are currently not supported
            unimplemented!();
        } else {
            let self_untyped = self.as_opaque();
            let other_untyped = other.as_opaque();
            // always lock the port with lower id first to prevent deadlock
            // (circular wait condition)
            let (a, b) = if self.id().0 < other.id().0 {
                (self_untyped, other_untyped)
            } else {
                (other_untyped, self_untyped)
            };
            let mut a_edge = a.edge.lock().unwrap();
            let mut b_edge = b.edge.lock().unwrap();
            if a_edge.as_ref().and_then(|x| x.upgrade()).is_some()
                || b_edge.as_ref().and_then(|x| x.upgrade()).is_some()
            {
                return Err(ConnectError::AlreadyConnected);
            }
            *a_edge = Some(Arc::downgrade(&b));
            *b_edge = Some(Arc::downgrade(&a));

            // UnsafeCells protected by edge mutex
            let self_connect_wait = unsafe { &mut *self.connect_wait.get() };
            let other_connect_wait = unsafe { &mut *other.connect_wait.get() };
            for waker in self_connect_wait
                .drain(..)
                .chain(other_connect_wait.drain(..))
            {
                waker.wake();
            }
            Ok(())
        }
    }

    /// Disconnect this port from another.
    /// Fails with ConnectError::NotConnected if the port is already disconnected.
    pub fn disconnect(self: &Arc<Port<I, O>>) -> Result<(), ConnectError> {
        // similarly to with `connect`, we need to lock the edges of the two ports in
        // a deterministic order to prevent a deadlock.
        // but here, we don't know the other port until we lock this port.
        // so, we read the other port with `edge()`, lock the two in the required order,
        // verify nothing changed in between reading and locking,
        // then finally clear the connection.
        // if verification fails we race again until it succeeds.
        loop {
            let other = &self.edge().ok_or(ConnectError::NotConnected)?;
            if other.id() == self.id() {
                // self edges are currently not supported
                unimplemented!();
            } else {
                let (mut self_edge, mut other_edge);
                if self.id().0 < other.id().0 {
                    self_edge = self.edge.lock().unwrap();
                    other_edge = other.edge.lock().unwrap();
                } else {
                    other_edge = other.edge.lock().unwrap();
                    self_edge = self.edge.lock().unwrap();
                };
                // check that the port this one is connected to hasn't changed in between
                // finding `other` and locking the edges
                if !self_edge
                    .as_ref()
                    .and_then(|x| x.upgrade())
                    .map(|self_other| Arc::ptr_eq(other, &self_other))
                    .unwrap_or(false)
                {
                    continue;
                }
                // other should definitely be connected to self if we made it here
                assert!(Arc::ptr_eq(
                    &other_edge.as_ref().unwrap().upgrade().unwrap(),
                    self
                ));
                *self_edge = None;
                *other_edge = None;

                // fail any waiting readers so that the task isn't left half finished across a
                // disconnect/reconnect
                self.disconnect_abort();
                other.disconnect_abort();
                break;
            }
        }
        Ok(())
    }
    fn disconnect_abort(&self) {
        loop {
            if self.buf_lock
                .compare_and_swap(false, true, Ordering::Acquire) == false
            {
                self.disconnect_occured.store(true, Ordering::SeqCst);
                let reader = self.reader_buf.take(Ordering::SeqCst);
                self.buf_lock.store(false, Ordering::Release);
                // wake any readers that were waiting, since they need to fail now
                reader.map(|reader| reader.wake());
                // wake anyone that was waiting for the critical section
                self.buf_lock_q.try_pop().map(|x| x.wake());
                break;
            }
        }
    }
    fn edge(&self) -> Option<Arc<Port<O, I>>> {
        self.edge.lock().unwrap().as_ref().and_then(|x| x.upgrade())
    }

    /// Returns a `Future` which writes a `Vec` of data to a port, returning the port.
    /// Writing cannot currently fail: TODO make the type signature reflect this.
    pub fn write(
        self: Arc<Port<I, O>>,
        data: Vec<O>,
    ) -> impl Future<Item = Arc<Port<I, O>>, Error = (Arc<Port<I, O>>, Error)> {
        WriteFuture {
            port: Some(self),
            data: typed_as_bytes(data.into()),
        }.fuse()
    }
    /// Write a single item. Equivalent to `write(vec![data])`
    pub fn write1(
        self: Arc<Port<I, O>>,
        data: O,
    ) -> impl Future<Item = Arc<Port<I, O>>, Error = (Arc<Port<I, O>>, Error)> {
        self.write(vec![data])
    }

    /// Returns a `Future` which reads all available data from a port, returning the port and the
    /// data. Succeeds when at least one item is available. Returns an error if the port has been
    /// disconnected since the task began.
    pub fn read(
        self: Arc<Port<I, O>>,
    ) -> impl Future<Item = (Arc<Port<I, O>>, Box<[I]>), Error = (Arc<Port<I, O>>, Error)> {
        ReadFuture {
            port: Some(self),
            n: None,
        }.fuse()
    }
    /// Read exactly n items from a port. Completes when at least n items become available. See
    /// `read` for more information.
    pub fn read_n(
        self: Arc<Port<I, O>>,
        n: usize,
    ) -> impl Future<Item = (Arc<Port<I, O>>, Box<[I]>), Error = (Arc<Port<I, O>>, Error)> {
        ReadFuture {
            port: Some(self),
            n: Some(n * mem::size_of::<I>()),
        }.fuse()
    }
    /// Equivalent to `read_n(1)`, but returns the item itself instead of a singleton array
    pub fn read1(
        self: Arc<Port<I, O>>,
    ) -> impl Future<Item = (Arc<Port<I, O>>, I), Error = (Arc<Port<I, O>>, Error)> {
        self.read_n(1)
            .map(|(port, data)| (port, data.into_vec().drain(..).next().unwrap()))
    }
}

pub struct ReadFuture<I: 'static, O: 'static> {
    port: Option<Arc<Port<I, O>>>,
    n: Option<usize>,
}

impl<I: 'static, O: 'static> Future for ReadFuture<I, O> {
    type Item = (Arc<Port<I, O>>, Box<[I]>);
    type Error = (Arc<Port<I, O>>, Error);
    fn poll(&mut self, cx: &mut Context) -> Result<Async<Self::Item>, Self::Error> {
        let mut data = None;
        let port = self.port.as_ref().unwrap();

        // attempt to enter critical section of buffer
        // we try to do it twice: if we fail the first time, then we put ourselves in the queue of
        // futures waiting to enter. then on the second time around, either we get in or we know
        // that we will be awoken.
        for try in 0..2 {
            // attempt to acquire buffer lock
            if port.buf_lock
                .compare_and_swap(false, true, Ordering::Acquire) == false
            {
                // if a disconnect has occured, then we fail the future so that the task isn't left
                // in a half finished state.
                if port.disconnect_occured
                    .compare_and_swap(true, false, Ordering::SeqCst)
                {
                    port.buf_lock.store(false, Ordering::Release); // leave critical section
                    return Err((self.port.take().unwrap(), Error::Disconnected));
                }
                // the buffer is protected by buf_lock
                let buf = unsafe { &mut *port.buffer.get() };
                // attempt read
                if self.n.map(|n| buf.len() < n).unwrap_or(buf.len() == 0) {
                    // not enough data available
                    // register to wake on next write
                    if let Some(old_reader) = port.reader_buf.swap(cx.waker(), Ordering::SeqCst) {
                        if !cx.waker().will_wake(&old_reader) {
                            // this might be supported in the future,
                            // if you want multiple threads working on items from one port.
                            // but it's probably better implemented at another level of
                            // abstraction.
                            // maybe we should have a list of active read futures, not waiting read
                            // futures?
                            panic!("multiple simultaneous reads from a port are not supported");
                        }
                    }
                    data = None;
                } else {
                    // move data out of queue
                    let n = self.n.unwrap_or(buf.len());
                    let iter = buf.drain(..n);
                    data = Some(iter.collect::<Vec<_>>().into());
                }
                // leave critical section
                port.buf_lock.store(false, Ordering::Release);
                break;
            } else {
                // couldn't lock buffer
                if try == 0 {
                    // first time around, register this future to be notified upon critical section exit
                    port.buf_lock_q.push(cx.waker());
                } else {
                    // on the second try, the above line has already run so we can yield
                    return Ok(Async::Pending);
                }
            }
        }

        // now that we are out of the critical section,
        // wake a future that was waiting for it, if any
        port.buf_lock_q.try_pop().map(|x| x.wake());

        if let Some(data) = data {
            Ok(Async::Ready((
                self.port.take().unwrap(),
                bytes_as_typed(data),
            )))
        } else {
            // the waker would have been put into reader_buf if we get here
            Ok(Async::Pending)
        }
    }
}

pub struct WriteFuture<I: 'static, O: 'static> {
    port: Option<Arc<Port<I, O>>>,
    data: Box<[u8]>,
}

impl<I: 'static, O: 'static> Future for WriteFuture<I, O> {
    type Item = Arc<Port<I, O>>;
    type Error = (Arc<Port<I, O>>, Error);
    fn poll(&mut self, cx: &mut Context) -> Result<Async<Self::Item>, Self::Error> {
        let port = self.port.as_ref().unwrap();
        let other = {
            let edge = port.edge.lock().unwrap().as_ref().and_then(|x| x.upgrade());
            match edge {
                Some(other) => other,
                None => {
                    // register to wake on connect
                    let connect_wait = unsafe { &mut *port.connect_wait.get() };
                    connect_wait.push(cx.waker());
                    return Ok(Async::Pending);
                }
            }
        };

        for try in 0..2 {
            // attempt to enter critical section of buffer
            if other
                .buf_lock
                .compare_and_swap(false, true, Ordering::Acquire) == false
            {
                let buf = unsafe { &mut *other.buffer.get() };
                buf.extend(self.data.into_iter());

                // leave critical section
                other.buf_lock.store(false, Ordering::Release);
                break;
            } else {
                // couldn't lock buffer
                // register this future to be notified upon critical section exit
                if try == 0 {
                    other.buf_lock_q.push(cx.waker());
                } else {
                    return Ok(Async::Pending);
                }
            }
        }

        // wake a future that was waiting for the critical section, if any
        other.buf_lock_q.try_pop().map(|x| x.wake());

        // wake any readers that are waiting for a write here
        other.reader_buf.take(Ordering::SeqCst).map(|x| x.wake());

        Ok(Async::Ready(self.port.take().unwrap()))
    }
}

#[derive(Debug)]
pub enum ConnectError {
    AlreadyConnected,
    TypeMismatch,
    NotConnected,
}

/// Error cases
#[derive(Debug)]
pub enum Error {
    NotConnected,
    InvalidNode,
    InvalidPort,
    NotAvailable,
    Disconnected,
}

fn typed_as_bytes<T: 'static>(data: Box<[T]>) -> Box<[u8]> {
    let size = data.len() * mem::size_of::<T>();
    let raw = Box::into_raw(data);
    unsafe { Box::from_raw(slice::from_raw_parts_mut(raw as *mut u8, size)) }
}

fn bytes_as_typed<T: 'static>(data: Box<[u8]>) -> Box<[T]> {
    assert_eq!(data.len() % mem::size_of::<T>(), 0); // ensure alignment
    let size = data.len() / mem::size_of::<T>();
    let raw = Box::into_raw(data);
    unsafe { Box::from_raw(slice::from_raw_parts_mut(raw as *mut T, size)) }
}
