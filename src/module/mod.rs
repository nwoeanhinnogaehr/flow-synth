pub mod audio_io;
pub mod debug;
pub mod flow;
pub mod livecode;

use futures::executor;
use std::sync::Arc;

pub trait Module: Send {
    fn new(ifc: Arc<flow::Interface>) -> Self
    where
        Self: Sized;
    fn name() -> &'static str
    where
        Self: Sized;
    fn start<Ex: executor::Executor>(&mut self, exec: Ex);
    fn stop(&mut self);
    fn ports(&self) -> Vec<Arc<flow::OpaquePort>>;
}
