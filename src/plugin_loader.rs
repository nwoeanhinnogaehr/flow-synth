use control::*;
use libloading::{Library, Symbol};
pub use libloading::Result;

#[derive(Debug)]
pub struct NodeLibrary {
    lib: Library,
    pub nodes: Vec<NodeDescriptor>,
    pub name: &'static str,
}

impl NodeLibrary {
    pub fn load(path: &str) -> Result<NodeLibrary> {
        let lib = Library::new(path)?;
        unsafe {
            let nodes = {
                let func: Symbol<fn() -> Vec<NodeDescriptor>> = lib.get(b"get_descriptors")?;
                func()
            };
            let name = {
                let func: Symbol<fn() -> &'static str> = lib.get(b"get_name")?;
                func()
            };
            Ok(NodeLibrary { lib, nodes, name })
        }
    }
}
