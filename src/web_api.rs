// TODO: restructure this file

use rocket;
use modular_flow::graph::{InPortID, NodeID, OutPortID};
use std::sync::Arc;
use rocket_contrib::{Json, Value};
use rocket::{Request, Response, State};
use rocket::http::Status;
use rocket::response::Responder;
use rocket_cors;
use control::*;
use serialize;
use std::thread;
use ws::listen;
use std::time::Duration;

struct WebApi {
    inst: Instance,
}

impl WebApi {
    fn new(inst: Instance) -> WebApi {
        WebApi { inst }
    }

    fn node(&self, id: NodeID) -> Result<Arc<NodeInstance>, JsonErr> {
        self.inst.nodes.node(id).ok_or(JsonErr(Json(json!("invalid node"))))
    }

    fn remove_node(&self, id: NodeID) -> Result<(), JsonErr> {
        // TODO error
        self.inst.nodes.remove(id);
        Ok(())
    }

    fn state_json(&self) -> JsonResult {
        let nodes = self.inst.nodes.nodes();
        let nodes: Vec<_> = nodes
            .iter()
            .map(|node| {
                // TODO better abstraction
                let in_ports: Vec<_> = node.ctl
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
                                    "id": idx,
                                    "name": port.name(),

                                })
                            })
                            .unwrap_or(json!({"id": idx, "name": port.name() }))
                    })
                    .collect();
                let out_ports: Vec<_> = node.ctl
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
                                    "id": idx,
                                    "name": port.name(),

                                })
                            })
                            .unwrap_or(json!({"id": idx, "name": port.name() }))
                    })
                    .collect();
                let message_descriptors: Vec<_> = node.ctl
                    .message_descriptors()
                    .iter()
                    .enumerate()
                    .map(|(idx, msg)| {
                        json!({
                            "id": idx,
                            "name": msg.name,
                            "args": msg.args.iter().map(|arg|
                                                        json!({
                                                            "name": arg.name,
                                                            "type": format!("{:?}", arg.ty),
                                                        })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                json!({
                    "id": node.ctl.node().id().0,
                    "type_name": node.type_name,
                    "ports": {
                        "in": in_ports,
                        "out": out_ports,
                    },
                    "status": status_string(node),
                    "message_descriptors": message_descriptors,
                })
            })
            .collect();
        let types: Vec<_> = self.inst.types
            .nodes()
            .iter()
            .map(|node| {
                json!({
                    "name": node.name
                })
            })
            .collect();
        let libs: Vec<_> = self.inst.types.libs().iter().map(|lib| {
            json!({
                "name": lib.name,
                "path": lib.path,
            })
        }).collect();
        resp_ok(json!({
            "nodes": nodes,
            "types": types,
            "libs": libs,
        }))
    }
}

#[get("/state")]
fn state_info(this: State<Arc<WebApi>>) -> JsonResult {
    this.state_json()
}

fn status_string(node: &NodeInstance) -> &'static str {
    if node.ctl.stopped() {
        "stopped"
    } else {
        "running"
    }
}

//TODO make this post so names can contain arbitrary characters
#[get("/type/new/<name>")]
fn node_create(this: State<Arc<WebApi>>, name: String) -> JsonResult {
    let desc = this.inst.types.node(&name).ok_or(JsonErr(Json(json!("id out of bounds"))))?;
    let ctl = (desc.new)(this.inst.ctx.clone(), NewNodeConfig { node: None });
    let id = ctl.node().id().0;
    this.inst.nodes.insert(NodeInstance {
        ctl,
        type_name: desc.name,
    });
    resp_ok(json!({
        "id": id,
    }))
}

#[get("/node/connect/<src_node_id>/<src_port_id>/to/<dst_node_id>/<dst_port_id>")]
fn connect_port(
    this: State<Arc<WebApi>>,
    src_node_id: usize,
    src_port_id: usize,
    dst_node_id: usize,
    dst_port_id: usize,
) -> JsonResult {
    match this.inst.ctx.graph().connect(
        NodeID(src_node_id),
        OutPortID(src_port_id),
        NodeID(dst_node_id),
        InPortID(dst_port_id),
    ) {
        Err(_) => resp_err(json!("cannot connect")),
        Ok(_) => resp_ok(json!({})),
    }
}

#[get("/node/disconnect/<node_id>/<port_id>")]
fn disconnect_port(this: State<Arc<WebApi>>, node_id: usize, port_id: usize) -> JsonResult {
    let node = this.inst.ctx.graph().node(NodeID(node_id)).map_err(|_| JsonErr(Json(json!("invalid node id"))))?;
    let port = node.in_port(InPortID(port_id)).map_err(|_| JsonErr(Json(json!("invalid port id"))))?;
    match this.inst.ctx.graph().disconnect_in(port) {
        Err(_) => resp_err(json!("cannot disconnect: already connected")),
        Ok(_) => resp_ok(json!({})),
    }
}
#[get("/node/kill/<node_id>")]
fn set_node_status(this: State<Arc<WebApi>>, node_id: usize) -> JsonResult {
    this.inst.kill_node(NodeID(node_id)).map_err(|_| JsonErr(Json(json!("couldn't kill node"))))?;
    resp_ok(json!({}))
}

#[post("/node/send_message/<node_id>/<message_id>", format = "application/json", data = "<args>")]
fn send_message(
    this: State<Arc<WebApi>>,
    node_id: usize,
    message_id: usize,
    args: Json<Vec<String>>,
) -> JsonResult {
    use self::message::*;
    let node = this.node(NodeID(node_id))?;
    let message_descriptor = &node.ctl.message_descriptors()[message_id];
    let args = args.0;
    if args.iter().count() != message_descriptor.args.len() {
        return resp_err(json!("wrong arg count"));
    }
    let parsed_args: Result<Vec<_>, _> = args.iter()
        .zip(message_descriptor.args.iter())
        .map(|(arg, desc)| {
            Ok(match desc.ty {
                Type::Bool => Value::Bool(arg.parse().map_err(|e| JsonErr(Json(json!(format!("{:?}", e)))))?),
                Type::Int => Value::Int(arg.parse().map_err(|e| JsonErr(Json(json!(format!("{:?}", e)))))?),
                Type::Float => {
                    Value::Float(arg.parse().map_err(|e| JsonErr(Json(json!(format!("{:?}", e)))))?)
                }
                Type::String => Value::String(arg.clone()),
            })
        })
        .collect();
    let message = Message {
        desc: message_descriptor.clone(),
        args: parsed_args?,
    };
    node.ctl.send_message(message);
    resp_ok(json!({}))
}

#[post("/type/reload_library", format = "application/json", data = "<path>")]
fn reload_library(this: State<Arc<WebApi>>, path: Json<String>) -> JsonResult {
    this.inst.reload_lib(&path);
    resp_ok(json!({}))
}

fn run_notifier(api: Arc<WebApi>) {
    thread::spawn(move || {
        let api = api.clone();
        listen("127.0.0.1:3012", move |out| {
            let api = api.clone();
            thread::spawn(move || {
                let mut prev_nodes_str = "".into();
                loop {
                    thread::sleep(Duration::from_millis(100));
                    let state_str =
                        format!("{}", api.state_json().map(|x| (x.0).0).unwrap_or_else(|x| (x.0).0));
                    if state_str != prev_nodes_str {
                        out.send(state_str.clone()).unwrap();
                        serialize::to_file("dump.json", &api.inst);
                        prev_nodes_str = state_str;
                    }
                }
            });
            |_| Ok(())
        }).unwrap()
    });
}

pub fn run_server(inst: Instance) {
    let api = Arc::new(WebApi::new(inst));
    run_notifier(api.clone());
    let options = rocket_cors::Cors::default();
    rocket::ignite()
        .mount(
            "/",
            routes![
                state_info,
                node_create,
                connect_port,
                disconnect_port,
                set_node_status,
                send_message,
                reload_library,
            ],
        )
        .manage(api)
        .attach(options)
        .launch();
}

#[derive(Debug)]
struct JsonErr(Json<Value>);
struct JsonOk(Json<Value>);

type JsonResult = Result<JsonOk, JsonErr>;

fn resp_err(data: Value) -> JsonResult {
    Err(JsonErr(Json(data)))
}
fn resp_ok(data: Value) -> JsonResult {
    Ok(JsonOk(Json(data)))
}

impl Responder<'static> for JsonOk {
    fn respond_to(self, req: &Request) -> Result<Response<'static>, Status> {
        let JsonOk(json) = self;
        let Json(value) = json;
        let out = Json(json!({
            "status": "ok",
            "data": value,
        }));
        out.respond_to(req)
    }
}

impl Responder<'static> for JsonErr {
    fn respond_to(self, req: &Request) -> Result<Response<'static>, Status> {
        let JsonErr(json) = self;
        let Json(value) = json;
        let out = Json(json!({
            "status": "err",
            "data": value,
        }));
        out.respond_to(req)
    }
}
