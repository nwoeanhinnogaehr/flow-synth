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
use std::sync::{Arc, Mutex};

use gfx;
use gfx::traits::{Factory, FactoryExt};
use gfx::{Bind, CommandBuffer, Device, Encoder, IntoIndexBuffer, PipelineState, Resources, Slice};
use gfx::memory::Usage;
use gfx::buffer::Role;
use gfx::handle::RenderTargetView;
use gfx_window_glutin as gfx_glutin;
use gfx_text;
use gfx_device_gl as gl;

type ColorFormat = gfx::format::Srgba8;
type DepthFormat = gfx::format::DepthStencil;

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
    let mut model = Model::new();
    model.graph.add_node(&mf::MetaModule::new("foo", |_| {
        Mutex::new(Box::new(GuiModuleWrapper::new(TestModule {})))
    }));
    main_loop(&mut model);
}

type Graph = mf::Graph<Mutex<Box<GuiModule>>>;

/// Model holds info about GUI state
/// and program state
struct Model {
    time: f32,
    graph: Arc<Graph>,
    window_size: [f32; 2],
    mouse_pos: [f32; 2],
}

impl Model {
    fn new() -> Model {
        Model {
            graph: Graph::new(),
            time: 0.0,
            window_size: [0.0, 0.0],
            mouse_pos: [0.0, 0.0],
        }
    }

    fn update(&mut self, event: &glutin::Event) {
        use glutin::WindowEvent::*;
        use glutin::*;
        //println!("{:?}", event);
        match event {
            glutin::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                Resized(w, h) => {
                    self.window_size = [*w as f32, *h as f32];
                }
                CursorMoved {
                    device_id: _,
                    position,
                } => {
                    self.mouse_pos =
                        pixels_to_coords(self.window_size, [position.0 as f32, position.1 as f32]);
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                } => match button {
                    MouseButton::Right => match state {
                        ElementState::Pressed => {}
                        ElementState::Released => {}
                    },

                    MouseButton::Left => match state {
                        ElementState::Pressed => {}
                        ElementState::Released => {}
                    },
                    _ => (),
                },
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
fn pixels_to_coords(size: [f32; 2], pix: [f32; 2]) -> [f32; 2] {
    [pix[0] / size[0] * 2.0 - 1.0, pix[1] / size[1] * -2.0 + 1.0]
}

fn main_loop(model: &mut Model) {
    // init window
    let mut events_loop = EventsLoop::new();
    let context = ContextBuilder::new();
    let builder = WindowBuilder::new().with_title(String::from("flow-synth"));
    let (window, mut device, mut factory, mut main_color, mut main_depth) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop);

    // init rendering pipeline
    let mut ctx = RenderContext::new(factory.clone(), main_color.clone());

    // begin main loop
    let mut running = true;
    let timer = Instant::now();
    let mut frames = VecDeque::new();
    while running {
        let now = timer.elapsed();
        model.time = now.as_secs() as f32 + now.subsec_nanos() as f32 / 1_000_000_000.0;
        frames.push_back(model.time);
        while let Some(&old_frame) = frames.front() {
            if old_frame < model.time - 1.0 {
                frames.pop_front();
            } else {
                break;
            }
        }
        events_loop.poll_events(|event| {
            model.update(&event);
            for node in model.graph.nodes() {
                let mut module = node.module().lock().unwrap();
                module.update(&model, &event);
            }
            use glutin::WindowEvent::*;
            match event {
                glutin::Event::WindowEvent {
                    window_id: _,
                    event,
                } => match event {
                    Closed => running = false,
                    Resized(_, _) => {
                        gfx_glutin::update_views(&window, &mut main_color, &mut main_depth);
                        ctx.set_target(main_color.clone());
                    }
                    _ => (),
                },
                _ => (),
            }
        });

        ctx.begin_frame();

        for node in model.graph.nodes() {
            let mut module = node.module().lock().unwrap();
            module.render(&mut ctx);
        }

        // debug text
        ctx.draw_text(
            &format!("FPS: {}", frames.len()),
            [0, 0],
            [1.0, 1.0, 1.0, 1.0],
        );
        ctx.draw_text(
            &format!("Time: {}", model.time),
            [0, 20],
            [1.0, 1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

struct RectRenderer {
    factory: gl::Factory,
    pso: PipelineState<gl::Resources, pipe::Meta>,
    slice: Slice<gl::Resources>,
    data: pipe::Data<gl::Resources>,
    rects: Vec<Rect>,
}
impl RectRenderer {
    fn new(mut factory: gl::Factory, target: RenderTargetView<gl::Resources, ColorFormat>) -> RectRenderer {
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
            out: target,
        };
        RectRenderer {
            factory,
            pso,
            slice,
            data,
            rects: Vec::new(),
        }
    }
    fn set_target(&mut self, target: RenderTargetView<gl::Resources, ColorFormat>) {
        self.data.out = target;
    }
    fn push(&mut self, rect: Rect) {
        self.rects.push(rect);
    }
    fn draw(&mut self, encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>) {
        self.data.instances = self.factory
            .create_buffer_immutable(&self.rects, Role::Vertex, Bind::empty())
            .unwrap();
        self.slice.instances = Some((self.rects.len() as u32, 0));
        encoder.draw(&self.slice, &self.pso, &self.data);
        self.rects.clear();
    }
}

struct RenderContext {
    factory: gl::Factory,
    encoder: Encoder<gl::Resources, gl::CommandBuffer>,
    text: gfx_text::Renderer<gl::Resources, gl::Factory>,
    target: RenderTargetView<gl::Resources, ColorFormat>,
    rects: RectRenderer,
}
impl RenderContext {
    fn new(mut factory: gl::Factory, target: RenderTargetView<gl::Resources, ColorFormat>) -> RenderContext {
        let encoder = factory.create_command_buffer().into();
        let text = gfx_text::new(factory.clone()).build().unwrap();
        let rects = RectRenderer::new(factory.clone(), target.clone());
        RenderContext {
            target,
            factory,
            encoder,
            text,
            rects,
        }
    }
    pub fn begin_frame(&mut self) {
        self.encoder.clear(&self.target, BLACK);
    }
    pub fn draw_text(&mut self, text: &str, pos: [i32; 2], color: [f32; 4]) {
        self.text.add(text, pos, color);
    }
    pub fn draw_rect(&mut self, rect: Rect) {
        self.rects.push(rect);
    }
    pub fn set_target(&mut self, target: RenderTargetView<gl::Resources, ColorFormat>) {
        self.rects.set_target(target.clone());
        self.target = target;
    }

    pub fn end_frame(&mut self, device: &mut gl::Device) {
        self.rects.draw(&mut self.encoder);
        self.text.draw(&mut self.encoder, &self.target);

        self.encoder.flush(device);
    }
}

trait Module {
    fn start(&mut self);
}
trait GuiModule: Module {
    fn update(&mut self, model: &Model, &glutin::Event);
    fn render(&mut self, &mut RenderContext);
}
struct GuiModuleWrapper<Module> {
    module: Module,

    rect: Rect,
    drag: Option<[f32; 2]>,
}

impl<Module> GuiModuleWrapper<Module> {
    fn new(module: Module) -> GuiModuleWrapper<Module> {
        GuiModuleWrapper {
            module,
            rect: Rect {
                translate: [0.0, 0.0],
                scale: [0.1, 0.1],
                color: [1.0, 1.0, 1.0],
            },
            drag: None,
        }
    }
}

impl<T> GuiModule for GuiModuleWrapper<T>
where
    T: Module,
{
    fn render(&mut self, ctx: &mut RenderContext) {
        ctx.draw_rect(self.rect);
    }
    fn update(&mut self, model: &Model, event: &glutin::Event) {
        if let Some(drag) = self.drag {
            self.rect.translate = [-drag[0] + model.mouse_pos[0], -drag[1] + model.mouse_pos[1]];
        }

        use glutin::WindowEvent::*;
        use glutin::*;
        match event {
            glutin::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                MouseInput {
                    device_id: _,
                    state,
                    button,
                } => match button {
                    MouseButton::Left => match state {
                        ElementState::Pressed => {
                            if point_in_rect(model.mouse_pos, &self.rect) {
                                self.drag = Some([
                                    model.mouse_pos[0] - self.rect.translate[0],
                                    model.mouse_pos[1] - self.rect.translate[1],
                                ]);
                                self.rect.color = [1.0, 0.0, 0.0];
                            }
                        }
                        ElementState::Released => {
                            if point_in_rect(model.mouse_pos, &self.rect) {
                                self.drag = None;
                                self.rect.color = [1.0, 1.0, 1.0];
                            }
                        }
                    },
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        }
    }
}
impl<T> Module for GuiModuleWrapper<T>
where
    T: Module,
{
    fn start(&mut self) {
        T::start(&mut self.module);
    }
}

struct TestModule {}
impl Module for TestModule {
    fn start(&mut self) {
        println!("start!!");
    }
}
