// TODO: restructure this file

use rocket;
use modular_flow::context::Context;
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
    ctx: Arc<Context>,
    inst: InstanceList,
    desc: DescriptorList,
}

impl WebApi {
    fn new(ctx: Arc<Context>, desc: DescriptorList, inst: InstanceList) -> WebApi {
        WebApi { ctx, inst, desc }
    }

    fn node(&self, id: NodeID) -> Result<Arc<NodeInstance>, JsonErr> {
        self.inst.node(id).ok_or(JsonErr(Json(json!("invalid node"))))
    }

    fn remove_node(&self, id: NodeID) -> Result<(), JsonErr> {
        // TODO error
        self.inst.remove(id);
        Ok(())
    }

    fn nodes_json(&self) -> JsonResult {
        let nodes = self.inst.nodes();
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
                    "name": node.name,
                    "ports": {
                        "in": in_ports,
                        "out": out_ports,
                    },
                    "status": status_string(node),
                    "message_descriptors": message_descriptors,
                })
            })
            .collect();
        resp_ok(json!(nodes))
    }
}

#[get("/type")]
fn type_list(this: State<Arc<WebApi>>) -> JsonResult {
    let types: Vec<_> = this.desc
        .nodes()
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            json!({
                "name": node.name
            })
        })
        .collect();
    resp_ok(json!(types))
}

#[get("/node")]
fn node_list(this: State<Arc<WebApi>>) -> JsonResult {
    this.nodes_json()
}

fn status_string(node: &NodeInstance) -> &'static str {
    if node.ctl.stopped() {
        "stopped"
    } else {
        "running"
    }
}

//TODO make this post so names can contain arbitrary characters
#[get("/type/<name>/new")]
fn node_create(this: State<Arc<WebApi>>, name: String) -> JsonResult {
    let desc = this.desc.node(&name).ok_or(JsonErr(Json(json!("id out of bounds"))))?;
    let ctl = (desc.new)(this.ctx.clone(), NewNodeConfig { node: None });
    let id = ctl.node().id().0;
    this.inst.insert(NodeInstance {
        ctl,
        name: desc.name,
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
    match this.ctx.graph().connect(
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
    let node = this.ctx.graph().node(NodeID(node_id)).map_err(|_| JsonErr(Json(json!("invalid node id"))))?;
    let port = node.in_port(InPortID(port_id)).map_err(|_| JsonErr(Json(json!("invalid port id"))))?;
    match this.ctx.graph().disconnect_in(port) {
        Err(_) => resp_err(json!("cannot disconnect: already connected")),
        Ok(_) => resp_ok(json!({})),
    }
}
#[get("/node/kill/<node_id>")]
fn set_node_status(this: State<Arc<WebApi>>, node_id: usize) -> JsonResult {
    let node = this.node(NodeID(node_id))?;
    node.ctl.stop();
    node.ctl.node().subscribe();
    while node.ctl.node().attached() {
        thread::park(); // TODO: relying on implementation detail
    }
    node.ctl.node().unsubscribe();
    this.ctx
        .graph()
        .remove_node(NodeID(node_id))
        .map_err(|_| JsonErr(Json(json!("couldn't remove node from graph"))))?;
    this.remove_node(NodeID(node_id))
        .map_err(|_| JsonErr(Json(json!("couldn't remove node from active node list"))))?;
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

fn run_notifier(api: Arc<WebApi>) {
    thread::spawn(move || {
        let api = api.clone();
        listen("127.0.0.1:3012", move |out| {
            let api = api.clone();
            thread::spawn(move || {
                let mut prev_nodes_str = "".into();
                loop {
                    thread::sleep(Duration::from_millis(100));
                    let nodes_str =
                        format!("{}", api.nodes_json().map(|x| (x.0).0).unwrap_or_else(|x| (x.0).0));
                    if nodes_str != prev_nodes_str {
                        out.send(nodes_str.clone()).unwrap();
                        serialize::to_file("dump.json", &api.ctx, &api.inst);
                        prev_nodes_str = nodes_str;
                    }
                }
            });
            |_| Ok(())
        }).unwrap()
    });
}

pub fn run_server(ctx: Arc<Context>, desc_list: DescriptorList, inst_list: InstanceList) {
    let api = Arc::new(WebApi::new(ctx, desc_list, inst_list));
    run_notifier(api.clone());
    let options = rocket_cors::Cors::default();
    rocket::ignite()
        .mount(
            "/",
            routes![
                type_list,
                node_list,
                node_create,
                connect_port,
                disconnect_port,
                set_node_status,
                send_message,
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
