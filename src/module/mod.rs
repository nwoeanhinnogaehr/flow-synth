pub mod debug;

use std::sync::Arc;
use modular_flow as mf;
use futures::executor;

pub trait Module: Send {
    fn new(ifc: Arc<mf::Interface>) -> Self
    where
        Self: Sized;
    fn name() -> &'static str
    where
        Self: Sized;
    fn start<Ex: executor::Executor>(&mut self, exec: Ex);
    fn ports(&self) -> Vec<Arc<mf::Port>>;
}
