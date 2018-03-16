/*!
 * This is some kind of a library for dataflow computation. It's still very experimental and may
 * become something completely different in the end.
 */

use future_ext::Lock;

use futures::prelude::*;
use futures::task::Context;

use std::sync::{Arc, RwLock, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    // it is UNSAFE to use the type parameters I and O to store real data here
    // because of how OpaquePort is implemented!
    // only PhantomData is ok!
    _in: PhantomData<I>,
    _out: PhantomData<O>,
    in_ty: TypeId,
    out_ty: TypeId,

    name: String,
    id: PortId,
    inner: Lock<PortInner>,
    edge: Lock<Edge<I, O>>,
}

struct PortInner {
    buffer: VecDeque<u8>,
    disconnect_occured: bool,
    read_wait: Vec<task::Waker>,
}

struct Edge<I: 'static, O: 'static> {
    other: Option<Weak<Port<O, I>>>,
    connect_wait: Vec<task::Waker>,
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
            inner: Lock::new(PortInner {
                buffer: VecDeque::new(),
                disconnect_occured: false,
                read_wait: Vec::new(),
            }),
            edge: Lock::new(Edge {
                other: None,
                connect_wait: Vec::new(),
            }),
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
            let mut a_edge = a.edge.spin_lock();
            let mut b_edge = b.edge.spin_lock();
            if a_edge.other.as_ref().and_then(|x| x.upgrade()).is_some()
                || b_edge.other.as_ref().and_then(|x| x.upgrade()).is_some()
            {
                return Err(ConnectError::AlreadyConnected);
            }
            a_edge.other = Some(Arc::downgrade(&b));
            b_edge.other = Some(Arc::downgrade(&a));

            // UnsafeCells protected by edge mutex
            for waker in a_edge.connect_wait
                .drain(..)
                .chain(b_edge.connect_wait.drain(..))
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
                let (mut a_edge, mut b_edge);
                if self.id().0 < other.id().0 {
                    a_edge = self.edge.spin_lock();
                    b_edge = other.edge.spin_lock();
                } else {
                    b_edge = other.edge.spin_lock();
                    a_edge = self.edge.spin_lock();
                };
                // check that the port this one is connected to hasn't changed in between
                // finding `other` and locking the edges
                if !a_edge.other
                    .as_ref()
                    .and_then(|x| x.upgrade())
                    .map(|self_other| Arc::ptr_eq(other, &self_other))
                    .unwrap_or(false)
                {
                    continue;
                }
                // other should definitely be connected to self if we made it here
                assert!(Arc::ptr_eq(
                    &b_edge.other.as_ref().unwrap().upgrade().unwrap(),
                    self
                ));
                a_edge.other = None;
                b_edge.other = None;

                drop(a_edge);
                drop(b_edge);

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
        let readers;
        {
            let mut inner = self.inner.spin_lock();
            inner.disconnect_occured = true;
            readers = inner.read_wait.drain(..).collect::<Vec<_>>();
        };

        // wake any readers that were waiting, since they need to fail now
        for reader in readers {
            reader.wake();
        }
    }
    fn edge(&self) -> Option<Arc<Port<O, I>>> {
        self.edge.spin_lock().other.as_ref().and_then(|x| x.upgrade())
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
            other: None,
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
        let port = self.port.as_ref().unwrap();
        let data;

        {
            let mut inner = match port.inner.lock().poll(cx) {
                Ok(Async::Ready(inner)) => inner,
                Ok(Async::Pending) => return Ok(Async::Pending),
            };
            // if a disconnect has occured, then we fail the future so that the task isn't left
            // in a half finished state.
            if inner.disconnect_occured {
                inner.disconnect_occured = false;
                drop(inner);
                return Err((self.port.take().unwrap(), Error::Disconnected));
            }
            // the buffer is protected by buf_lock
            let buf = &mut inner.buffer;
            // attempt read
            if self.n.map(|n| buf.len() < n).unwrap_or(buf.len() == 0) {
                // not enough data available
                // register to wake on next write
                inner.read_wait.push(cx.waker().clone());
                data = None;
            } else {
                // move data out of queue
                let n = self.n.unwrap_or(buf.len());
                let iter = buf.drain(..n);
                data = Some(iter.collect::<Vec<_>>().into());
            }
        }

        if let Some(data) = data {
            Ok(Async::Ready((
                self.port.take().unwrap(),
                bytes_as_typed(data),
            )))
        } else {
            // the waker would have been put into inner.read_wait if we get here
            Ok(Async::Pending)
        }
    }
}

pub struct WriteFuture<I: 'static, O: 'static> {
    port: Option<Arc<Port<I, O>>>,
    data: Box<[u8]>,
    other: Option<Arc<Port<O, I>>>,
}

impl<I: 'static, O: 'static> Future for WriteFuture<I, O> {
    type Item = Arc<Port<I, O>>;
    type Error = (Arc<Port<I, O>>, Error);
    fn poll(&mut self, cx: &mut Context) -> Result<Async<Self::Item>, Self::Error> {
        if self.other.is_none() {
            let port = self.port.as_ref().unwrap();
            self.other = Some({
                let mut edge = match port.edge.lock().poll(cx) {
                    Ok(Async::Ready(edge)) => edge,
                    Ok(Async::Pending) => return Ok(Async::Pending),
                };
                match edge.other.as_ref().and_then(|x| x.upgrade()) {
                    Some(other) => other,
                    None => {
                        // register to wake on connect
                        edge.connect_wait.push(cx.waker().clone());
                        return Ok(Async::Pending);
                    }
                }
            });
        }
        let other = self.other.as_ref().unwrap();

        let readers;
        {
            let mut inner = match other.inner.lock().poll(cx) {
                Ok(Async::Ready(inner)) => inner,
                Ok(Async::Pending) => return Ok(Async::Pending),
            };
            let buf = &mut inner.buffer;
            buf.extend(self.data.into_iter());
            readers = inner.read_wait.drain(..).collect::<Vec<_>>();
        }

        // wake any readers that are waiting for a write here
        for reader in readers {
            reader.wake();
        }

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
