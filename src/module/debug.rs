use super::Module;

use modular_flow as mf;

use std::sync::Arc;

pub struct TestModule {
    ifc: Arc<mf::Interface>,
}
impl Module for TestModule {
    fn new(ifc: Arc<mf::Interface>) -> TestModule {
        ifc.add_port(&mf::MetaPort::new::<u8, u8, _>("TestPort1"));
        ifc.add_port(&mf::MetaPort::new::<u8, u8, _>("TestPort2"));
        TestModule {
            ifc,
        }
    }
    fn start(&mut self) {
        println!("TestModule start!!");
    }
    fn title(&self) -> String {
        "TestModule".into()
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}
