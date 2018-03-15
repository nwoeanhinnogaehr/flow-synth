pub mod flow;
pub mod debug;
pub mod audio_io;

use std::sync::Arc;
use futures::executor;

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
