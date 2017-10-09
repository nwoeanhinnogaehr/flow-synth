use std::thread::{self, Thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use modular_flow::graph::*;
use modular_flow::context::Context;
use plugin_loader::{self, NodeLibrary};

pub struct Instance {
    pub ctx: Arc<Context>,
    pub nodes: NodeInstances,
    pub types: NodeDescriptors,
}

impl Instance {
    pub fn new() -> Instance {
        Instance {
            ctx: Arc::new(Context::new(Graph::new())),
            nodes: NodeInstances::new(),
            types: NodeDescriptors::new(),
        }
    }
}

#[derive(Clone)]
pub struct NodeInstance {
    pub ctl: Arc<RemoteControl>,
    pub name: &'static str,
}

pub struct NodeInstances {
    list: Mutex<Vec<Arc<NodeInstance>>>,
}

impl NodeInstances {
    pub fn new() -> NodeInstances {
        NodeInstances {
            list: Mutex::new(Vec::new()),
        }
    }
    pub fn insert(&self, inst: NodeInstance) {
        self.list.lock().unwrap().push(Arc::new(inst));
    }
    pub fn remove(&self, id: NodeID) {
        self.list.lock().unwrap().retain(|node| node.ctl.node().id() != id);
    }
    pub fn node(&self, id: NodeID) -> Option<Arc<NodeInstance>> {
        self.list.lock().unwrap().iter().cloned().find(|node| node.ctl.node().id() == id)
    }
    pub fn nodes(&self) -> Vec<Arc<NodeInstance>> {
        self.list.lock().unwrap().clone()
    }
}

pub struct NewNodeConfig {
    pub node: Option<NodeID>, // node id to attach to
}

#[derive(Clone, Debug)]
pub struct NodeDescriptor {
    pub name: &'static str,
    pub new: fn(Arc<Context>, NewNodeConfig) -> Arc<RemoteControl>,
}

pub struct NodeDescriptors {
    libs: Mutex<Vec<NodeLibrary>>,
}

impl NodeDescriptors {
    pub fn new() -> NodeDescriptors {
        NodeDescriptors {
            libs: Mutex::new(vec![]),
        }
    }
    pub fn lib_paths(&self) -> Vec<String> {
        self.libs.lock().unwrap().iter().map(|lib| lib.path.clone()).collect()
    }
    pub fn load_library(&self, path: &str) -> plugin_loader::Result<&'static str> {
        let lib = NodeLibrary::load(path)?;
        let name = lib.name;
        self.libs.lock().unwrap().push(lib);
        Ok(name)
    }
    pub fn node(&self, name: &str) -> Option<NodeDescriptor> {
        self.nodes().iter().cloned().find(|node| node.name == name)
    }
    pub fn nodes(&self) -> Vec<NodeDescriptor> {
        self.libs.lock().unwrap().iter().flat_map(|lib| lib.nodes.clone()).collect()
    }
}

pub mod message {
    #[derive(Clone, Debug)]
    pub enum Type {
        Bool,
        Int,
        Float,
        String,
    }
    #[derive(Clone, Debug)]
    pub enum Value {
        Bool(bool),
        Int(i64),
        Float(f64),
        String(String),
    }
    #[derive(Clone, Debug)]
    pub struct ArgDesc {
        pub name: String,
        pub ty: Type,
    }
    #[derive(Clone, Debug)]
    pub struct Desc {
        pub name: &'static str,
        pub args: Vec<ArgDesc>,
    }
    #[derive(Clone, Debug)]
    pub struct Message {
        pub desc: Desc,
        pub args: Vec<Value>,
    }
}

pub struct RemoteControl {
    ctx: Arc<Context>,
    node: Arc<Node>,
    stop_thread: Mutex<Option<Thread>>,
    stopped: AtomicBool,
    messages: Vec<message::Desc>,
    msg_queue: Mutex<VecDeque<message::Message>>,
}
impl RemoteControl {
    pub fn new(ctx: Arc<Context>, node: Arc<Node>, messages: Vec<message::Desc>) -> RemoteControl {
        RemoteControl {
            ctx,
            node,
            stop_thread: Mutex::new(None),
            stopped: AtomicBool::new(false),
            messages,
            msg_queue: Mutex::new(VecDeque::new()),
        }
    }
    pub fn message_descriptors(&self) -> &[message::Desc] {
        &self.messages
    }
    pub fn send_message(&self, msg: message::Message) {
        self.msg_queue.lock().unwrap().push_back(msg);
        self.node.set_aborting(true);
        self.node.notify();
    }
    pub fn recv_message(&self) -> Option<message::Message> {
        self.msg_queue.lock().unwrap().pop_front()
    }
    /// dangerous, assumes only one thread!! FIXME
    pub fn block_until_stopped(&self) {
        *self.stop_thread.lock().unwrap() = Some(thread::current());
        while !self.stopped.load(Ordering::Acquire) {
            thread::park();
        }
    }
    pub fn stop(&self) {
        self.node.set_aborting(true);
        self.stopped.store(true, Ordering::Release);
        self.stop_thread.lock().unwrap().as_ref().map(|thread| thread.unpark());
    }
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub fn node(&self) -> &Node {
        &*self.node
    }
    pub fn context(&self) -> Arc<Context> {
        self.ctx.clone()
    }
}
