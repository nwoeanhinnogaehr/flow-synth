use control::*;
use libloading::{Library, Result, Symbol};

#[derive(Debug)]
pub struct NodeLibrary {
    lib: Library,
    nodes: Vec<NodeDescriptor>,
}

pub fn load(path: &str) -> Result<NodeLibrary> {
    let lib = Library::new(path)?;
    unsafe {
        let nodes = {
            let func: Symbol<fn() -> Vec<NodeDescriptor>> = lib.get(b"get_descriptors")?;
            func()
        };
        Ok(NodeLibrary { lib, nodes })
    }
}
