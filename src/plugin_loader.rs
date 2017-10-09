use control::*;
use libloading::{Library, Symbol};
pub use libloading::Result;

#[derive(Debug)]
pub struct NodeLibrary {
    lib: Library,
    pub nodes: Vec<NodeDescriptor>,
    pub name: String,
    pub path: String,
    pub file_path: String,
}

impl NodeLibrary {
    // path/file_path hack is because we need to force dlopen to reload
    // so path gets copied to file_path before loading,
    // then we actually load file_path,
    // and delete it afterwards.
    //
    // reason it isn't being unloaded seems to be because pthread also dlopens anything
    // we dlopen that starts a thread, then doesn't dlclose as quickly as we would like it to.
    pub fn load(path: &str, file_path: &str) -> Result<NodeLibrary> {
        let lib = Library::new(file_path)?;
        unsafe {
            let nodes = {
                let func: Symbol<fn() -> Vec<NodeDescriptor>> = lib.get(b"get_descriptors")?;
                func()
            };
            let name = {
                let func: Symbol<fn() -> String> = lib.get(b"get_name")?;
                func()
            };
            println!("loaded library {:?} from {}", name, file_path);
            Ok(NodeLibrary { lib, nodes, name, path: path.into(), file_path: file_path.into() })
        }
    }
}

impl Drop for NodeLibrary {
    fn drop(&mut self) {
        println!("dropping lib {:?} from {}", self.name, self.file_path);
    }
}
