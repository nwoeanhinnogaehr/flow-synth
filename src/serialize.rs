use modular_flow::context::Context;
use modular_flow::graph::*;
use control::{NewNodeConfig, NodeInstance, InstanceList, DescriptorList};
use serde_json;
use std::io::{Read, Write};
use std::fs::File;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct InstanceDesc {
    name: String,
    id: NodeID,
}
#[derive(Serialize)]
struct Container<'a> {
    inst_desc: Vec<InstanceDesc>,
    graph: &'a Graph,
}
#[derive(Deserialize)]
struct OwningContainer {
    inst_desc: Vec<InstanceDesc>,
    graph: Graph,
}
pub fn to_string(ctx: &Context, inst: &InstanceList) -> String {
    let container = Container {
        inst_desc: inst.nodes()
            .iter()
            .map(|node| {
                InstanceDesc {
                    name: node.name.into(),
                    id: node.ctl.node().id(),
                }
            })
        .collect(),
        graph: ctx.graph(),
    };
    serde_json::to_string(&container).unwrap()
}
pub fn from_string(serialized: String, desc: &DescriptorList) -> (Arc<Context>, InstanceList) {
    let container: OwningContainer = serde_json::from_str(&serialized).unwrap();
    let OwningContainer { inst_desc, graph } = container;
    let ctx = Arc::new(Context::new(graph));
    let inst = InstanceList::new();
    for it in inst_desc {
        let node_desc = desc.node(&it.name).expect("node desc not loaded");
        let node_inst = NodeInstance {
            ctl: (node_desc.new)(ctx.clone(), NewNodeConfig { node: Some(it.id) }),
            name: node_desc.name,
        };
        inst.insert(node_inst);
    }
    (ctx, inst)
}
pub fn to_file(name: &str, ctx: &Context, inst: &InstanceList) {
    let mut file = File::create(name).unwrap();
    let string = to_string(ctx, inst);
    file.write_all(string.as_bytes()).unwrap();
}
pub fn from_file(name: &str, desc: &DescriptorList) -> (Arc<Context>, InstanceList) {
    let mut file = File::open(name).unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();
    from_string(string, desc)
}
