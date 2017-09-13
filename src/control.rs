use std::thread::{self, Thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use modular_flow::context::Context;
use modular_flow::graph::*;

pub trait NodeDescriptor {
    const NAME: &'static str;
    fn new(Arc<Context>) -> Box<NodeInstance>;
}

pub trait NodeInstance: Send + Sync {
    fn title(&self) -> String {
        "".into()
    }
    fn run(&mut self) -> Arc<RemoteControl>;
    fn node(&self) -> &Node;
}

pub enum ControlState {
    Running,
    Paused,
    Stopped,
}
pub struct RemoteControl {
    pause_thread: Mutex<Option<Thread>>,
    stop_thread: Mutex<Option<Thread>>,
    paused: AtomicBool,
    stopped: AtomicBool,
}
impl RemoteControl {
    pub fn new() -> RemoteControl {
        RemoteControl {
            pause_thread: Mutex::new(None),
            stop_thread: Mutex::new(None),
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }
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
}
