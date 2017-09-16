use rocket;
use modular_flow::context::Context;
use modular_flow::graph::*;
use std::sync::Arc;
use rocket_contrib::{Json, Value};
use rocket::State;
use audio_io;
use stft;
use std::sync::RwLock;
use rocket_cors;
use control::*;

struct StaticNode {
    name: &'static str,
    make: fn(Arc<Context>) -> Box<NodeInstance>,
}

const TYPES: &'static [StaticNode] = &[
    StaticNode {
        name: audio_io::AudioIO::NAME,
        make: audio_io::AudioIO::new,
    },
    StaticNode {
        name: stft::Stft::NAME,
        make: stft::Stft::new,
    },
];

struct ActiveNode {
    node: Box<NodeInstance>,
    ctl: Option<Arc<RemoteControl>>,
    static_node: &'static StaticNode,
}

struct WebApi {
    ctx: Arc<Context>,
    nodes: RwLock<Vec<ActiveNode>>,
}

impl WebApi {
    fn new(ctx: Arc<Context>) -> WebApi {
        WebApi {
            ctx,
            nodes: RwLock::new(Vec::new()),
        }
    }
}

#[get("/type")]
fn type_list() -> Json<Value> {
    let types: Vec<_> = TYPES
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            json!({
                "id": idx,
                "name": node.name
            })
        })
        .collect();
    Json(json!(types))
}

#[get("/node")]
fn node_list(this: State<WebApi>) -> Json<Value> {
    let nodes = this.nodes.read().unwrap();
    let nodes: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            // TODO better abstraction
            let in_ports: Vec<_> = node.node
                .node()
                .in_ports()
                .iter()
                .enumerate()
                .map(|(idx, port)| {
                    port.edge()
                        .map(|edge| {
                            json!({
                                "edge": {
                                    "node": edge.node.0,
                                    "port": edge.port.0,
                                },
                                "id": idx
                            })
                        })
                        .unwrap_or(json!({"id": idx}))
                })
                .collect();
            let out_ports: Vec<_> = node.node
                .node()
                .out_ports()
                .iter()
                .enumerate()
                .map(|(idx, port)| {
                    port.edge()
                        .map(|edge| {
                            json!({
                                "edge": {
                                    "node": edge.node.0,
                                    "port": edge.port.0,
                                },
                                "id": idx
                            })
                        })
                        .unwrap_or(json!({"id": idx}))
                })
                .collect();
            let message_descriptors: Vec<_> = node.ctl
                .as_ref()
                .map(|ctl| {
                    ctl.message_descriptors()
                        .iter()
                        .map(|msg| {
                            json!({
                                "name": msg.name,
                                "args": msg.args.iter().map(|arg| format!("{:?}", arg)).collect::<Vec<_>>(),
                            })
                        })
                        .collect()
                })
                .unwrap_or(Vec::new());
            json!({
                "id": idx,
                "title": node.node.title(),
                "name": node.static_node.name,
                "ports": {
                    "in": in_ports,
                    "out": out_ports,
                },
                "status": status_string(node),
                "message_descriptors": message_descriptors,
            })
        })
        .collect();
    Json(json!(nodes))
}

fn status_string(node: &ActiveNode) -> &'static str {
    match node.ctl {
        Some(ref ctl) => {
            let status = ctl.poll();
            match status {
                ControlState::Paused => "paused",
                ControlState::Running => "running",
                ControlState::Stopped => "stopped",
            }
        }
        None => "stopped",
    }
}

#[get("/type/<type_id>/new")]
fn node_create(this: State<WebApi>, type_id: usize) -> Json<Value> {
    if type_id >= TYPES.len() {
        return json_err("id out of bounds");
    }
    let node = (TYPES[type_id].make)(this.ctx.clone());
    let id = node.node().id().0;
    this.nodes.write().unwrap().push(ActiveNode {
        node,
        ctl: None,
        static_node: &TYPES[type_id],
    });
    json_ok(json!({
        "id": id,
    }))
}

#[get("/node/connect/<src_node_id>/<src_port_id>/to/<dst_node_id>/<dst_port_id>")]
fn node_info(
    this: State<WebApi>,
    src_node_id: usize,
    src_port_id: usize,
    dst_node_id: usize,
    dst_port_id: usize,
) -> Json<Value> {
    match this.ctx.graph().connect(
        NodeID(src_node_id),
        OutPortID(src_port_id),
        NodeID(dst_node_id),
        InPortID(dst_port_id),
    ) {
        Err(_) => json_err("cannot connect"),
        Ok(_) => json_ok(json!({})),
    }
}

#[get("/node/<node_id>/set_status/<status>")]
fn set_node_status(this: State<WebApi>, node_id: usize, status: String) -> Json<Value> {
    let mut nodes = this.nodes.write().unwrap();
    if node_id >= nodes.len() {
        return json_err("node id out of bounds");
    }
    let node = &mut nodes[node_id];
    match status.as_ref() {
        "run" => match node.ctl {
            Some(ref ctl) => {
                let status = ctl.poll();
                match status {
                    ControlState::Paused => {
                        ctl.resume();
                        json_ok(json!({}))
                    }
                    _ => json_err("cannot run from this state"),
                }
            }
            None => {
                node.ctl = Some(node.node.run());
                json_ok(json!({}))
            }
        },
        "pause" => match node.ctl {
            Some(ref ctl) => {
                let status = ctl.poll();
                match status {
                    ControlState::Running => {
                        ctl.pause();
                        json_ok(json!({}))
                    }
                    _ => json_err("cannot pause from this state"),
                }
            }
            None => json_err("node not started"),
        },
        _ => json_err("invalid status"),
    }
}

pub fn run_server(ctx: Arc<Context>) {
    let options = rocket_cors::Cors::default();
    rocket::ignite()
        .mount("/", routes![type_list, node_list, node_create, node_info, set_node_status])
        .manage(WebApi::new(ctx))
        .attach(options)
        .launch();
}

fn json_err<S: AsRef<str>>(msg: S) -> Json<Value> {
    assert!(msg.as_ref() != "ok");
    Json(json!({
        "status": msg.as_ref()
    }))
}

fn json_ok(mut msg: Value) -> Json<Value> {
    if let Value::Object(ref mut map) = msg {
        map.insert("status".into(), "ok".into());
    }
    Json(msg)
}
