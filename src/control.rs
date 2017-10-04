use std::thread::{self, Thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use modular_flow::context::Context;
use modular_flow::graph::*;

#[derive(Debug)]
pub struct StaticNode {
    pub name: &'static str,
    pub make: fn(Arc<Context>) -> Arc<RemoteControl>,
}

pub trait NodeDescriptor {
    const NAME: &'static str;
    fn new(Arc<Context>) -> Arc<RemoteControl>;
    fn describe() -> StaticNode {
        StaticNode {
            name: Self::NAME,
            make: Self::new,
        }
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
