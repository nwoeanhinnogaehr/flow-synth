use modular_flow::context::Context;
use modular_flow::graph::*;
use control::{Instance, NewNodeConfig, NodeInstance, NodeInstances, NodeDescriptors};
use serde_json;
use std::io::{Read, Write};
use std::fs::File;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct InstanceDesc {
    type_name: String,
    id: NodeID,
}
#[derive(Serialize, Deserialize)]
struct LibraryDesc {
    path: String,
}
#[derive(Serialize)]
struct Container<'a> {
    inst_desc: Vec<InstanceDesc>,
    libs: Vec<LibraryDesc>,
    graph: &'a Graph,
}
#[derive(Deserialize)]
struct OwningContainer {
    inst_desc: Vec<InstanceDesc>,
    libs: Vec<LibraryDesc>,
    graph: Graph,
}
pub fn to_string(inst: &Instance) -> String {
    let container = Container {
        inst_desc: inst.nodes.nodes()
            .iter()
            .map(|node| {
                InstanceDesc {
                    type_name: node.type_name.clone(),
                    id: node.ctl.node().id(),
                }
            })
        .collect(),
        libs: inst.types.libs().iter().map(|lib| LibraryDesc { path: lib.path.clone() }).collect(),
        graph: inst.ctx.graph(),
    };
    serde_json::to_string(&container).unwrap()
}
pub fn from_string(serialized: String) -> Instance {
    let container: OwningContainer = serde_json::from_str(&serialized).unwrap();
    let OwningContainer { inst_desc, libs, graph } = container;
    let ctx = Arc::new(Context::new(graph));
    let types = NodeDescriptors::new();
    let nodes = NodeInstances::new();
    for lib in libs {
        types.load_library(&lib.path).unwrap();
    }
    for it in inst_desc {
        let node_desc = types.node(&it.type_name).expect("node desc not loaded");
        let node_inst = NodeInstance {
            ctl: (node_desc.new)(ctx.clone(), NewNodeConfig { node: Some(it.id) }),
            type_name: node_desc.name,
        };
        nodes.insert(node_inst);
    }
    Instance {
        ctx,
        nodes,
        types,
    }
}
pub fn to_file(name: &str, inst: &Instance) {
    let mut file = File::create(name).unwrap();
    let string = to_string(inst);
    file.write_all(string.as_bytes()).unwrap();
}
pub fn from_file(name: &str) -> Instance {
    let mut file = File::open(name).unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();
    from_string(string)
}
