use std::thread::{self, Thread};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::time::Duration;
use std::fs;
use modular_flow::graph::*;
use modular_flow::context::Context;
use plugin_loader::{self, NodeLibrary};
use serde_json;
use serde::ser::Serialize;
use serde::de::DeserializeOwned;
use libc;

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
    pub fn load_lib(&self, path: &str) {
        {
            let libs = self.types.libs();
            let old_lib = libs.iter().find(|lib| lib.path == path);
            if let Some(old_lib) = old_lib {
                for node in self.nodes.nodes() {
                    if old_lib.nodes.iter().find(|desc| desc.name == node.type_name).is_some() {
                        self.stop_node(node.ctl.node().id()).unwrap();
                    }
                }
            }
        }
        let lib = self.types.load_library(path).unwrap();
        for node in self.nodes.nodes() {
            if let Some(node_desc) = lib.nodes.iter().find(|desc| desc.name == node.type_name) {
                self.nodes.remove(node.ctl.node().id());
                let ctl = (node_desc.new)(
                    self.ctx.clone(),
                    NewNodeConfig::from(node.ctl.node().id(), node.ctl.saved_data()),
                );
                self.nodes.insert(NodeInstance {
                    ctl,
                    type_name: node.type_name.clone(),
                });
            }
        }
    }
    pub fn reload_node(&self, id: NodeID) -> Result<()> {
        let node = self.nodes.node(id).ok_or(Error::InvalidNode)?;
        let libs = self.types.libs();
        let old_lib =
            libs.iter().find(|lib| lib.nodes.iter().find(|ty| ty.name == node.type_name).is_some()).unwrap();
        self.stop_node(node.ctl.node().id())?;
        let lib = self.types.load_library(&old_lib.path).unwrap();
        let node_desc = lib.nodes.iter().find(|desc| desc.name == node.type_name).unwrap();
        self.nodes.remove(node.ctl.node().id());
        let ctl = (node_desc.new)(
            self.ctx.clone(),
            NewNodeConfig::from(node.ctl.node().id(), node.ctl.saved_data()),
        );
        self.nodes.insert(NodeInstance {
            ctl,
            type_name: node.type_name.clone(),
        });
        Ok(())
    }
    pub fn kill_node(&self, id: NodeID) -> Result<()> {
        self.stop_node(id)?;
        self.ctx.graph().remove_node(id)?;
        self.nodes.remove(id);
        Ok(())
    }
    pub fn stop_node(&self, id: NodeID) -> Result<Arc<NodeInstance>> {
        let node = self.nodes.node(id).ok_or(Error::InvalidNode)?;
        println!("trying to stop {} [{}]", node.type_name, node.ctl.node().id().0);
        node.ctl.stop();
        while node.ctl.node().attached() {
            node.ctl.node().notify_self();
            thread::sleep(Duration::from_millis(25));
        }
        node.ctl.node().flush(self.ctx.graph());
        println!("stopped {} [{}]", node.type_name, node.ctl.node().id().0);
        Ok(node)
    }
}

#[derive(Clone)]
pub struct NodeInstance {
    pub ctl: Arc<RemoteControl>,
    pub type_name: String,
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
    pub saved_data: String,
}

impl NewNodeConfig {
    pub fn empty() -> NewNodeConfig {
        NewNodeConfig {
            node: None,
            saved_data: "".into(),
        }
    }
    pub fn from(node: NodeID, saved_data: String) -> NewNodeConfig {
        NewNodeConfig {
            node: Some(node),
            saved_data: saved_data,
        }
    }
}

#[derive(Clone)]
pub struct NodeDescriptor {
    pub name: String,
    pub new: Arc<Fn(Arc<Context>, NewNodeConfig) -> Arc<RemoteControl> + Send + Sync + 'static>,
}

impl NodeDescriptor {
    pub fn new<
        S: Into<String>,
        F: Fn(Arc<Context>, NewNodeConfig) -> Arc<RemoteControl> + Send + Sync + 'static,
    >(
        name: S,
        new: F,
    ) -> NodeDescriptor {
        NodeDescriptor {
            name: name.into(),
            new: Arc::new(new),
        }
    }
}

pub struct NodeDescriptors {
    load_count: AtomicUsize,
    libs: Mutex<Vec<Arc<NodeLibrary>>>,
    old_libs: Mutex<Vec<Arc<NodeLibrary>>>,
}

impl NodeDescriptors {
    pub fn new() -> NodeDescriptors {
        NodeDescriptors {
            load_count: AtomicUsize::new(0),
            libs: Mutex::new(vec![]),
            old_libs: Mutex::new(vec![]),
        }
    }
    pub fn libs(&self) -> Vec<Arc<NodeLibrary>> {
        self.libs.lock().unwrap().clone()
    }
    /// Replaces existing library with the same path if it exists
    pub fn load_library(&self, path: &str) -> plugin_loader::Result<Arc<NodeLibrary>> {
        let mut libs = self.libs.lock().unwrap();
        let new_path = format!(
            "{}{}-{}",
            path,
            self.load_count.fetch_add(1, Ordering::SeqCst),
            unsafe { libc::getpid() }
        );
        fs::copy(path, &new_path).unwrap();
        let new_lib = Arc::new(NodeLibrary::load(path, &new_path)?);
        if let Some(old_lib_idx) = libs.iter_mut().position(|lib| lib.path == path) {
            let old_lib = libs.swap_remove(old_lib_idx);
            self.old_libs.lock().unwrap().push(old_lib);
            //fs::remove_file(&old_lib.file_path).unwrap();
        }
        libs.push(new_lib.clone());
        Ok(new_lib)
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
        Usize,
        F32,
        String,
    }
    #[derive(Clone, Debug)]
    pub enum Value {
        Bool(bool),
        Usize(usize),
        F32(f32),
        String(String),
    }
    #[derive(Clone, Debug)]
    pub struct ArgDesc {
        pub name: String,
        pub ty: Type,
    }
    #[derive(Clone, Debug)]
    pub struct Desc {
        pub name: String,
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
    saved_data: Mutex<String>,
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
            saved_data: Mutex::new("".into()),
        }
    }
    pub fn message_descriptors(&self) -> &[message::Desc] {
        &self.messages
    }
    pub fn send_message(&self, msg: message::Message) {
        self.msg_queue.lock().unwrap().push_back(msg);
        self.node.set_aborting(true);
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
        self.stopped.store(true, Ordering::Release);
        self.node.set_aborting(true);
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
    pub fn save<T: Serialize>(&self, val: T) -> ::std::result::Result<(), serde_json::Error> {
        *self.saved_data.lock().unwrap() = serde_json::to_string(&val)?;
        Ok(())
    }
    pub fn restore<T: DeserializeOwned>(&self) -> ::std::result::Result<T, serde_json::Error> {
        serde_json::from_str(&self.saved_data.lock().unwrap())
    }
    pub fn saved_data(&self) -> String {
        self.saved_data.lock().unwrap().clone()
    }
    pub fn set_saved_data(&self, data: &str) {
        *self.saved_data.lock().unwrap() = data.into();
    }
}
