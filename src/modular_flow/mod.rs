/*!
 * This is some kind of a library for dataflow computation. It's still very experimental and may
 * become something completely different in the end.
 *
 * The end goal is to use it for procedural and generative art. It's inspired by Pure Data and
 * Max/MSP, but will probably have less focus on graphical programming. Modular live coding,
 * perhaps?
 *
 * This is iteration #2.
 */

use std::sync::{Arc, Condvar, Mutex, RwLock, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::{HashMap, VecDeque};
use std::mem;
use std::slice;
use std::any::TypeId;
use std::borrow::Cow;

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
    pub fn find_port(&self, name: &'static str) -> Option<Arc<Port>> {
        self.ifc.find_port(name)
    }
    /// Get a vector of references to all associated ports at the time of the call.
    pub fn ports(&self) -> Vec<Arc<Port>> {
        self.ifc.ports()
    }
}

/// The private interface for a module. The module is provided with an `Interface` upon construction.
/// An `Interface` has a superset of the functionality of a `Node`. It can be used to manipulate the
/// associated Ports.
pub struct Interface {
    id: NodeId,
    ports: RwLock<HashMap<PortId, Arc<Port>>>,
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
    pub fn find_port(&self, name: &str) -> Option<Arc<Port>> {
        self.ports
            .read()
            .unwrap()
            .iter()
            .find(|&(_, port)| port.meta.name == name)
            .map(|port| port.1)
            .cloned()
    }
    /// Get a vector of references to all associated ports at the time of the call.
    pub fn ports(&self) -> Vec<Arc<Port>> {
        self.ports.read().unwrap().values().cloned().collect()
    }
    /// Add a new port using the given metadata.
    pub fn add_port(&self, meta: &MetaPort) -> Arc<Port> {
        let port = Port::new(&self.graph.upgrade().unwrap(), meta);
        self.ports.write().unwrap().insert(port.id, port.clone());
        port
    }
    /// Remove a port by ID.
    pub fn remove_port(&self, port: PortId) -> Result<Arc<Port>, Error> {
        self.ports
            .write()
            .unwrap()
            .remove(&port)
            .ok_or(Error::InvalidPort)
    }

    /// Wait (block) until a `Signal` is received on a port.
    pub fn wait(&self, port: &Port) -> Signal {
        port.wait()
    }
    /// Get a vector of all unread Signals on a port.
    pub fn poll(&self, port: &Port) -> Vec<Signal> {
        port.poll()
    }
    /// Write data to a port.
    pub fn write<D: 'static>(&self, port: &Port, data: impl Into<Box<[D]>>) -> Result<(), Error> {
        port.write(data.into())
    }
    /// Read all available data from a port.
    pub fn read<D: 'static>(&self, port: &Port) -> Result<Box<[D]>, Error> {
        port.read()
    }
    /// Read exactly `n` values from a port.
    pub fn read_n<D: 'static>(&self, port: &Port, n: usize) -> Result<Box<[D]>, Error> {
        port.read_n(n)
    }
}

/// Port metadata.
#[derive(Clone)]
pub struct MetaPort {
    name: Cow<'static, str>,
    in_ty: TypeId,
    out_ty: TypeId,
}

impl MetaPort {
    /// Construct new port metadata with the given datatype and name.
    pub fn new<InT: 'static, OutT: 'static, N: Into<Cow<'static, str>>>(name: N) -> MetaPort {
        // sending ZSTs doesn't really make sense,
        // and will cause all kinds of confusing behavior like having
        // an infinite number of items available to read
        assert!(mem::size_of::<InT>() != 0);
        assert!(mem::size_of::<OutT>() != 0);
        MetaPort {
            name: name.into(),
            in_ty: TypeId::of::<InT>(),
            out_ty: TypeId::of::<OutT>(),
        }
    }
    /// Get the port name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Ports are the connection points of modules. They can be connected one-to-one with other ports,
/// and allow a single type of data (runtime checked) to flow bidirectionally.
///
/// TODO think about interactions/problems with multiple graphs
pub struct Port {
    meta: MetaPort,
    id: PortId,
    buffer: Mutex<VecDeque<u8>>,
    edge: RwLock<Option<Weak<Port>>>,
    signal: Mutex<VecDeque<Signal>>,
    cvar: Condvar,
}

impl Port {
    fn new(graph: &Graph, meta: &MetaPort) -> Arc<Port> {
        Arc::new(Port {
            meta: MetaPort::clone(meta),
            id: PortId(graph.generate_id()),
            buffer: Mutex::new(VecDeque::new()),
            edge: RwLock::new(None),
            signal: Mutex::new(VecDeque::new()),
            cvar: Condvar::new(),
        })
    }

    /// Get the associated metadata.
    pub fn meta(&self) -> &MetaPort {
        &self.meta
    }
    /// Get the PortId.
    pub fn id(&self) -> PortId {
        self.id
    }
    /// Connect this port to another.
    /// Fails with ConnectError::TypeMismatch if the ports have different data types.
    /// Fails with ConnectError::AlreadyConnected if either port is already connected.
    pub fn connect(self: &Arc<Port>, other: &Arc<Port>) -> Result<(), ConnectError> {
        if self.meta.in_ty != other.meta.out_ty || self.meta.out_ty != other.meta.in_ty {
            return Err(ConnectError::TypeMismatch);
        }
        if Arc::ptr_eq(self, other) {
            // self edges are currently not supported
            unimplemented!();
        } else {
            // always lock the port with lower id first to prevent deadlock
            // (circular wait condition)
            let (a, b) = if self.id().0 < other.id().0 {
                (self, other)
            } else {
                (other, self)
            };
            let mut a_edge = a.edge.write().unwrap();
            let mut b_edge = b.edge.write().unwrap();
            if a_edge.as_ref().and_then(|x| x.upgrade()).is_some()
                || b_edge.as_ref().and_then(|x| x.upgrade()).is_some()
            {
                return Err(ConnectError::AlreadyConnected);
            }
            *a_edge = Some(Arc::downgrade(b));
            *b_edge = Some(Arc::downgrade(a));
            a.signal(Signal::Connect);
            b.signal(Signal::Connect);
            Ok(())
        }
    }

    /// Disconnect this port from another.
    /// Fails with ConnectError::NotConnected if the port is already disconnected.
    pub fn disconnect(self: &Arc<Port>) -> Result<(), ConnectError> {
        // similarly to with `connect`, we need to lock the edges of the two ports in
        // a deterministic order to prevent a deadlock.
        // but here, we don't know the other port until we lock this port.
        // so, we read the other port with `edge()`, lock the two in the required order,
        // verify nothing changed in between reading and locking,
        // then finally clear the connection.
        // if verification fails we race again until it succeeds.
        loop {
            let other = &self.edge().ok_or(ConnectError::NotConnected)?;
            if Arc::ptr_eq(other, self) {
                // self edges are currently not supported
                unimplemented!();
            } else {
                let (mut self_edge, mut other_edge);
                if self.id().0 < other.id().0 {
                    self_edge = self.edge.write().unwrap();
                    other_edge = other.edge.write().unwrap();
                } else {
                    other_edge = other.edge.write().unwrap();
                    self_edge = self.edge.write().unwrap();
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
                self.signal(Signal::Disconnect);
                other.signal(Signal::Disconnect);
                break Ok(());
            }
        }
    }

    fn edge(&self) -> Option<Arc<Port>> {
        self.edge.read().unwrap().clone().and_then(|x| x.upgrade())
    }
    fn signal(&self, signal: Signal) {
        let mut lock = self.signal.lock().unwrap();
        lock.push_back(signal);
        self.cvar.notify_all();
    }
    fn poll(&self) -> Vec<Signal> {
        let mut lock = self.signal.lock().unwrap();
        let iter = lock.drain(..);
        iter.collect()
    }
    fn wait(&self) -> Signal {
        let mut lock = self.signal.lock().unwrap();
        while lock.is_empty() {
            lock = self.cvar.wait(lock).unwrap();
        }
        lock.pop_front().unwrap()
    }
    fn write<T: 'static>(&self, data: impl Into<Box<[T]>>) -> Result<(), Error> {
        assert!(self.meta.out_ty == TypeId::of::<T>());
        let bytes = typed_as_bytes(data.into());
        let other = self.edge().ok_or(Error::NotConnected)?;
        let mut buf = other.buffer.lock().unwrap();
        buf.extend(bytes.into_iter());
        other.signal(Signal::Write);
        Ok(())
    }
    fn read<T: 'static>(&self) -> Result<Box<[T]>, Error> {
        assert!(self.meta.in_ty == TypeId::of::<T>());
        let mut buf = self.buffer.lock().unwrap();
        let iter = buf.drain(..);
        let out = iter.collect::<Vec<_>>().into();
        Ok(bytes_as_typed(out))
    }
    fn read_n<T: 'static>(&self, n: usize) -> Result<Box<[T]>, Error> {
        assert!(self.meta.in_ty == TypeId::of::<T>());
        let mut buf = self.buffer.lock().unwrap();
        let n = n * mem::size_of::<T>();
        if n > buf.len() {
            return Err(Error::NotAvailable);
        }
        let iter = buf.drain(..n);
        let out = iter.collect::<Vec<_>>().into();
        Ok(bytes_as_typed(out))
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
}

/// Events occuring on a `Port`. Accessible via an `Interface`.
#[derive(Debug, PartialEq, Eq)]
pub enum Signal {
    Abort,
    Write,
    Connect,
    Disconnect,
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
