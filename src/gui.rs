use glutin::{self, Api, ContextBuilder, ControlFlow, EventsLoop, GlContext, GlRequest, Window,
             WindowBuilder, WindowEvent};

use modular_flow as mf;
use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use gfx;
use gfx::traits::{Factory, FactoryExt};
use gfx::{Bind, CommandBuffer, Device, Encoder, IntoIndexBuffer, Resources, Slice};
use gfx::memory::Usage;
use gfx::buffer::Role;
use gfx_window_glutin as gfx_glutin;
use gfx_text;

pub type ColorFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

gfx_defines! {
    vertex Vertex {
        pos: [f32; 2] = "a_Pos",
    }

    vertex Rect {
        translate: [f32; 2] = "a_Translate",
        scale: [f32; 2] = "a_Scale",
        color: [f32; 3] = "a_Color",
    }

    pipeline pipe {
        time: gfx::Global<f32> = "i_Time",
        vertices: gfx::VertexBuffer<Vertex> = (),
        instances: gfx::InstanceBuffer<Rect> = (),
        out: gfx::RenderTarget<ColorFormat> = "Target0",
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> [Vertex; 4] {
    [
        Vertex { pos: [x, y] },
        Vertex { pos: [x, y + h] },
        Vertex {
            pos: [x + w, y + h],
        },
        Vertex { pos: [x + w, y] },
    ]
}

const RECT_IDX: [u16; 6] = [0, 1, 2, 0, 2, 3];

pub fn gui_main() {
    let graph = mf::Graph::<Box<GuiModule>>::new();
    let node = graph.add_node(&mf::MetaModule {
        id: "test",
        new: |ifc| Box::new(TestModule{}) as Box<GuiModule>,
    });
    let node2 = graph.add_node(&mf::MetaModule {
        id: "test2",
        new: |ifc| Box::new(TestModule2{}) as Box<GuiModule>,
    });
    let mut guard = node.module();
    let mut guard2 = node2.module();
    let module = guard.as_mut().unwrap();
    let module2 = guard2.as_mut().unwrap();
    module.start();
    module.render();
    module2.start();
    module2.render();
    let mut model = Model::new();
    main_loop(&mut model);
}

/// Model holds info about GUI state
/// and program state
struct Model {
    nodes: Vec<Node>,
}

impl Model {
    fn new() -> Model {
        Model { nodes: Vec::new() }
    }
}

struct Node {
    id: mf::NodeId,
    rect: Rect,
    drag: Option<[f32; 2]>,
}

impl Node {
    fn new(inst: Rect) -> Node {
        Node {
            id: mf::NodeId(0),
            rect: inst,
            drag: None,
        }
    }
}

struct ModelUpdater {
    mouse_pos: [f32; 2],
    size: [f32; 2],
}
impl ModelUpdater {
    fn new() -> ModelUpdater {
        ModelUpdater {
            mouse_pos: [0.0, 0.0],
            size: [0.0, 0.0],
        }
    }
    fn pixels_to_coords(&self, pix: [f32; 2]) -> [f32; 2] {
        [
            pix[0] / self.size[0] * 2.0 - 1.0,
            pix[1] / self.size[1] * -2.0 + 1.0,
        ]
    }
    fn update(&mut self, model: &mut Model, event: &glutin::Event) {
        use glutin::WindowEvent::*;
        use glutin::*;
        //println!("{:?}", event);
        match event {
            glutin::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                Resized(w, h) => {
                    self.size = [*w as f32, *h as f32];
                }
                CursorMoved {
                    device_id: _,
                    position,
                } => {
                    self.mouse_pos = self.pixels_to_coords([position.0 as f32, position.1 as f32]);

                    // update nodes that we are dragging
                    for node in &mut model.nodes {
                        if let Some(drag) = node.drag {
                            node.rect.translate =
                                [-drag[0] + self.mouse_pos[0], -drag[1] + self.mouse_pos[1]];
                        }
                    }
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                } => {
                    match button {
                        // right button spawns nodes
                        MouseButton::Right => match state {
                            ElementState::Pressed => {
                                model.nodes.push(Node::new(Rect {
                                    translate: self.mouse_pos,
                                    scale: [0.1, 0.1],
                                    color: [0.0, 0.0, 1.0],
                                }));
                            }
                            ElementState::Released => {}
                        },

                        // left button drags nodes
                        MouseButton::Left => match state {
                            ElementState::Pressed => for node in &mut model.nodes {
                                if point_in_rect(self.mouse_pos, &node.rect) {
                                    node.drag = Some([
                                        self.mouse_pos[0] - node.rect.translate[0],
                                        self.mouse_pos[1] - node.rect.translate[1],
                                    ]);
                                    node.rect.color = [1.0, 0.0, 0.0];
                                }
                            },
                            ElementState::Released => for node in &mut model.nodes {
                                node.drag = None;
                                node.rect.color = [0.0, 0.0, 1.0];
                            },
                        },
                        _ => (),
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }
}
fn point_in_rect(pos: [f32; 2], rect: &Rect) -> bool {
    pos[0] >= rect.translate[0] && pos[0] <= rect.translate[0] + rect.scale[0] && pos[1] >= rect.translate[1]
        && pos[1] <= rect.translate[1] + rect.scale[1]
}

fn main_loop(model: &mut Model) {
    let mut updater = ModelUpdater::new();


    // init window
    let mut events_loop = EventsLoop::new();
    let context = ContextBuilder::new();
    let builder = WindowBuilder::new().with_title(String::from("flow-synth"));
    let (window, mut device, mut factory, main_color, mut main_depth) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop);


    // init rendering pipeline
    let mut encoder: gfx::Encoder<_, _> = factory.create_command_buffer().into();
    let pso = factory
        .create_pipeline_simple(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/rect_150.glslv"
            )),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/rect_150.glslf"
            )),
            pipe::new(),
        )
        .unwrap();
    let instance_buffer = factory
        .create_buffer(0, Role::Vertex, Usage::Data, Bind::empty())
        .unwrap(); // initially no instances. we create a new instance buffer each frame
    let vertex_buffer = factory
        .create_buffer_immutable(&rect(0.0, 0.0, 1.0, 1.0), Role::Vertex, Bind::empty())
        .unwrap();
    let index_buffer = RECT_IDX[..].into_index_buffer(&mut factory);
    let buffer_length = match index_buffer {
        gfx::IndexBuffer::Auto => vertex_buffer.len(),
        gfx::IndexBuffer::Index16(ref ib) => ib.len(),
        gfx::IndexBuffer::Index32(ref ib) => ib.len(),
    };
    let mut slice = Slice {
        start: 0,
        end: buffer_length as u32,
        base_vertex: 0,
        instances: Some((0, 0)),
        buffer: index_buffer,
    };
    let mut data = pipe::Data {
        time: 0.0,
        vertices: vertex_buffer,
        instances: instance_buffer,
        out: main_color,
    };
    let mut text = gfx_text::new(factory.clone()).build().unwrap();


    // begin main loop
    let mut running = true;
    let timer = Instant::now();
    let mut frames = VecDeque::new();
    while running {
        // handle events
        let now = timer.elapsed();
        data.time = now.as_secs() as f32 + now.subsec_nanos() as f32 / 1_000_000_000.0;
        frames.push_back(data.time);
        while let Some(&old_frame) = frames.front() {
            if old_frame < data.time - 1.0 {
                frames.pop_front();
            } else {
                break;
            }
        }
        events_loop.poll_events(|event| {
            updater.update(model, &event);
            use glutin::WindowEvent::*;
            match event {
                glutin::Event::WindowEvent {
                    window_id: _,
                    event,
                } => match event {
                    Closed => running = false,
                    Resized(_, _) => {
                        gfx_glutin::update_views(&window, &mut data.out, &mut main_depth);
                    }
                    _ => (),
                },
                _ => (),
            }
        });


        // begin frame
        encoder.clear(&data.out, BLACK);


        // render all rects
        let mut rects = Vec::new();
        for node in &model.nodes {
            rects.push(node.rect);
        }
        data.instances = factory
            .create_buffer_immutable(&rects, Role::Vertex, Bind::empty())
            .unwrap();
        slice.instances = Some((rects.len() as u32, 0));
        encoder.draw(&slice, &pso, &data);


        // render all text
        text.add(
            &format!("FPS: {}", frames.len()),
            [0, 0],
            [1.0, 1.0, 1.0, 1.0],
        );
        text.add(
            &format!("Time: {}", data.time),
            [0, 20],
            [1.0, 1.0, 1.0, 1.0],
        );
        text.draw(&mut encoder, &data.out);


        // finish frame
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

trait Module {
    fn start(&mut self);
}

trait GuiModule: Module {
    fn render(&mut self);
}

impl<T> GuiModule for T where T: Module {
    default fn render(&mut self) {
        println!("rendering!!");
    }
}
impl GuiModule for TestModule {
    fn render(&mut self) {
        println!("spec rendering!!");
    }
}
impl Module for TestModule {
    fn start(&mut self) {
        println!("start!!");
    }
}
impl Module for TestModule2 {
    fn start(&mut self) {
        println!("start2!!");
    }
}

struct TestModule {}
struct TestModule2 {}
