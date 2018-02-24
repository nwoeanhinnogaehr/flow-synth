pub mod debug;

use std::sync::Arc;
use modular_flow as mf;

pub trait Module {
    fn new(ifc: Arc<mf::Interface>) -> Self
    where
        Self: Sized;
    fn start(&mut self);
    fn title(&self) -> String;
    fn ports(&self) -> Vec<Arc<mf::Port>>;
}
