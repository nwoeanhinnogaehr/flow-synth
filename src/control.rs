use std::thread::{self, Thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use modular_flow::context::Context;
use modular_flow::graph::*;

pub trait NodeDescriptor {
    const NAME: &'static str;
    fn new(Arc<Context>) -> Arc<RemoteControl>;
}

#[derive(Debug)]
pub enum MessageArgType {
    Bool,
    Int,
    Float,
    String,
}
pub enum MessageArg {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}
pub struct MessageDescriptor {
    pub name: &'static str,
    pub args: Vec<MessageArgType>,
}
pub struct Message {
    pub desc: MessageDescriptor,
    pub args: Vec<MessageArg>,
}

pub enum ControlState {
    Running,
    Paused,
    Stopped,
}
pub struct RemoteControl {
    node: Arc<Node>,
    pause_thread: Mutex<Option<Thread>>,
    stop_thread: Mutex<Option<Thread>>,
    paused: AtomicBool,
    stopped: AtomicBool,
    messages: Vec<MessageDescriptor>,
    msg_queue: Mutex<VecDeque<Message>>,
}
impl RemoteControl {
    pub fn new(node: Arc<Node>, messages: Vec<MessageDescriptor>) -> RemoteControl {
        RemoteControl {
            node,
            pause_thread: Mutex::new(None),
            stop_thread: Mutex::new(None),
            paused: AtomicBool::new(true), // paused by default to allow initialization
            stopped: AtomicBool::new(false),
            messages,
            msg_queue: Mutex::new(VecDeque::new()),
        }
    }
    pub fn message_descriptors(&self) -> &[MessageDescriptor] {
        &self.messages
    }
    pub fn send_message(&self, msg: Message) {
        self.msg_queue.lock().unwrap().push_back(msg);
    }
    pub fn recv_message(&self) -> Option<Message> {
        self.msg_queue.lock().unwrap().pop_front()
    }
    /**
     * Never returns `ControlState::Paused`, instead blocking until control is resumed.
     */
    pub fn poll_state_blocking(&self) -> ControlState {
        *self.pause_thread.lock().unwrap() = Some(thread::current());
        if self.stopped.load(Ordering::Acquire) {
            return ControlState::Stopped;
        }
        assert!(self.pause_thread.lock().unwrap().as_ref().unwrap().id() == thread::current().id());
        while self.paused.load(Ordering::Acquire) {
            thread::park();
        }
        ControlState::Running
    }
    pub fn block_until_stopped(&self) {
        *self.stop_thread.lock().unwrap() = Some(thread::current());
        while !self.stopped.load(Ordering::Acquire) {
            thread::park();
        }
    }
    pub fn poll(&self) -> ControlState {
        if self.stopped.load(Ordering::Acquire) {
            ControlState::Stopped
        } else if self.paused.load(Ordering::Acquire) {
            ControlState::Paused
        } else {
            ControlState::Running
        }
    }

    // these should wait until the node has acknowledged the state change
    // before they return
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.pause_thread.lock().unwrap().as_ref().map(|thread| thread.unpark());
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.stop_thread.lock().unwrap().as_ref().map(|thread| thread.unpark());
    }

    pub fn node(&self) -> &Node {
        &*self.node
    }
}