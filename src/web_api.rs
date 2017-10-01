use rocket;
use modular_flow::context::Context;
use modular_flow::graph::{Port, InPortID, OutPortID, NodeID};
use std::sync::Arc;
use rocket_contrib::{Json, Value};
use rocket::{Request, Response, State};
use rocket::http::Status;
use rocket::response::Responder;
use audio_io;
use stft;
use pixel_scroller;
use std::sync::RwLock;
use rocket_cors;
use control::*;
use self::message;

#[derive(Debug)]
pub struct StaticNode {
    pub name: &'static str,
    pub make: fn(Arc<Context>) -> Arc<RemoteControl>,
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
    StaticNode {
        name: stft::IStft::NAME,
        make: stft::IStft::new,
    },
    StaticNode {
        name: stft::SpectrogramRender::NAME,
        make: stft::SpectrogramRender::new,
    },
    StaticNode {
        name: pixel_scroller::PixelScroller::NAME,
        make: pixel_scroller::PixelScroller::new,
    },
];

struct ActiveNode {
    ctl: Arc<RemoteControl>,
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
fn type_list() -> JsonResult {
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
    resp_ok(json!(types))
}

#[get("/node")]
fn node_list(this: State<WebApi>) -> JsonResult {
    let nodes = this.nodes.read().unwrap();
    let nodes: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| {
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
                                    "node": edge.node.id().0,
                                    "port": edge.port.id().0,
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
                                    "node": edge.node.id().0,
                                    "port": edge.port.id().0,
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
                "id": idx,
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
    resp_ok(json!(nodes))
}

fn status_string(node: &ActiveNode) -> &'static str {
    if node.ctl.stopped() {
        "stopped"
    } else {
        "running"
    }
}

#[get("/type/<type_id>/new")]
fn node_create(this: State<WebApi>, type_id: usize) -> JsonResult {
    if type_id >= TYPES.len() {
        return resp_err(json!("id out of bounds"));
    }
    let ctl = (TYPES[type_id].make)(this.ctx.clone());
    let id = ctl.node().id().0;
    this.nodes.write().unwrap().push(ActiveNode {
        ctl,
        static_node: &TYPES[type_id],
    });
    resp_ok(json!({
        "id": id,
    }))
}

#[get("/node/connect/<src_node_id>/<src_port_id>/to/<dst_node_id>/<dst_port_id>")]
fn connect_port(
    this: State<WebApi>,
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
fn disconnect_port(this: State<WebApi>, node_id: usize, port_id: usize) -> JsonResult {
    let node = this.ctx.graph().node(NodeID(node_id)).map_err(|_| JsonErr(Json(json!("invalid node id"))))?;
    let port = node.in_port(InPortID(port_id)).map_err(|_| JsonErr(Json(json!("invalid port id"))))?;
    match port.disconnect() {
        Err(_) => resp_err(json!("cannot disconnect: already connected")),
        Ok(_) => resp_ok(json!({})),
    }
}
// TODO this should probably just change into a thing to stop and delete nodes
#[get("/node/set_status/<node_id>/<status>")]
fn set_node_status(this: State<WebApi>, node_id: usize, status: String) -> JsonResult {
    let mut nodes = this.nodes.write().unwrap();
    if node_id >= nodes.len() {
        return resp_err(json!("node id out of bounds"));
    }
    let node = &mut nodes[node_id];
    resp_err(json!("unimplemented"))
}

#[post("/node/send_message/<node_id>/<message_id>", format = "application/json", data = "<args>")]
fn send_message(
    this: State<WebApi>,
    node_id: usize,
    message_id: usize,
    args: Json<Vec<String>>,
) -> JsonResult {
    use self::message::*;
    let mut nodes = this.nodes.write().unwrap();
    if node_id >= nodes.len() {
        return resp_err(json!("node id out of bounds"));
    }
    let node = &mut nodes[node_id];
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

pub fn run_server(ctx: Arc<Context>) {
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
        .manage(WebApi::new(ctx))
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
