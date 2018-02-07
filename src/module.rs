use std::sync::Arc;
use modular_flow as mf;

pub trait Module {
    fn start(&mut self);
    fn title(&self) -> String;
}

pub struct TestModule<M: mf::Module> {
    ifc: Arc<mf::Interface<M>>,
}
impl<M: mf::Module> TestModule<M> {
    pub fn new(ifc: Arc<mf::Interface<M>>) -> TestModule<M> {
        TestModule { ifc }
    }
}
impl<M: mf::Module> Module for TestModule<M> {
    fn start(&mut self) {
        println!("start!!");
    }
    fn title(&self) -> String {
        self.ifc.meta().name().to_string()
    }
}
