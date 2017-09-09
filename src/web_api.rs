use rocket;
use std::io;
use rocket::response::{Response, Stream};
use modular_flow::context::Context;
use modular_flow::graph::*;
use std::sync::Arc;
use rocket_contrib::{Json, Value};
use rocket::State;
use audio_io;
use std::sync::RwLock;
use rocket_cors;
use rocket::http::Method;
use rocket_cors::{AllowedHeaders, AllowedOrigins};
use super::control::*;

struct StaticNode {
    name: &'static str,
    make: fn(Arc<Context>) -> Box<NodeInstance>,
}

const TYPES: &'static [StaticNode] = &[
    StaticNode {
        name: audio_io::AudioIO::NAME,
        make: audio_io::AudioIO::new,
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
fn type_list(this: State<WebApi>) -> Json<Value> {
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
                .map(|port| {
                    port.edge()
                        .map(|edge| {
                            json!({
                                "edge": {
                                    "node": edge.node.0,
                                    "port": edge.port.0,
                                }
                            })
                        })
                        .unwrap_or(json!({}))
                })
                .collect();
            let out_ports: Vec<_> = node.node
                .node()
                .out_ports()
                .iter()
                .map(|port| {
                    port.edge()
                        .map(|edge| {
                            json!({
                                "edge": {
                                    "node": edge.node.0,
                                    "port": edge.port.0,
                                }
                            })
                        })
                        .unwrap_or(json!({}))
                })
                .collect();
            json!({
                "id": idx,
                "title": node.node.title(),
                "name": node.static_node.name,
                "ports": {
                    "in": in_ports,
                    "out": out_ports,
                },
                "status": "stopped", // TODO
            })
        })
        .collect();
    Json(json!(nodes))
}

#[get("/type/<type_id>/new")]
fn node_create(this: State<WebApi>, type_id: usize) -> Json<Value> {
    if type_id >= TYPES.len() {
        return Json(json!({"status": "id out of bounds"}));
    }
    let mut node = (TYPES[type_id].make)(this.ctx.clone());
    let id = node.node().id().0;
    this.nodes.write().unwrap().push(ActiveNode {
        node,
        ctl: None,
        static_node: &TYPES[type_id],
    });
    Json(json!({
        "status": "ok",
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
    this.ctx
        .graph()
        .connect(NodeID(src_node_id), OutPortID(src_port_id), NodeID(dst_node_id), InPortID(dst_port_id))
        .unwrap();
    Json(json!({
    }))
}

pub fn run_server(ctx: Arc<Context>) {
    let options = rocket_cors::Cors::default();
    rocket::ignite()
        .mount("/", routes![type_list, node_list, node_create, node_info])
        .manage(WebApi::new(ctx))
        .attach(options)
        .launch();
}
