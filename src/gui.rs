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
use gfx::texture;
use gfx::traits::{Factory, FactoryExt};
use gfx::{Bind, CommandBuffer, Device, Encoder, IntoIndexBuffer, PipelineState, Resources, Slice};
use gfx::memory::Usage;
use gfx::buffer::Role;
use gfx::handle::{Buffer, RenderTargetView, DepthStencilView, Sampler, ShaderResourceView, Texture};
use gfx_window_glutin as gfx_glutin;
use gfx_text;
use gfx_device_gl as gl;

type ColorFormat = gfx::format::Rgba8;
type DepthFormat = gfx::format::DepthStencil;

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

gfx_defines! {
    vertex Vertex {
        pos: [f32; 2] = "a_Pos",
    }

    vertex ColoredRect {
        translate: [f32; 3] = "a_Translate",
        scale: [f32; 2] = "a_Scale",
        color: [f32; 3] = "a_Color",
    }

    pipeline rect_pipe {
        aspect_ratio: gfx::Global<f32> = "i_AspectRatio",
        vertices: gfx::VertexBuffer<Vertex> = (),
        instances: gfx::InstanceBuffer<ColoredRect> = (),
        out: gfx::RenderTarget<ColorFormat> = "Target0",
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }

    vertex TexturedVertex {
        translate: [f32; 3] = "a_Translate",
        tex_coord: [f32; 2] = "a_TexCoord",
    }

    pipeline textured_rect_pipe {
        texture: gfx::TextureSampler<[f32; 4]> = "i_Texture",
        vertices: gfx::VertexBuffer<TexturedVertex> = (),
        out: gfx::RenderTarget<ColorFormat> = "Target0",
        depth: gfx::DepthTarget<DepthFormat> = gfx::state::Depth {
            fun: gfx::state::Comparison::LessEqual,
            write: true,
        },
    }
}

#[derive(Copy, Clone)]
struct Rect {
    translate: [f32; 3],
    scale: [f32; 2],
}

struct Target {
    color: RenderTargetView<gl::Resources, ColorFormat>,
    depth: DepthStencilView<gl::Resources, DepthFormat>,
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
    main_loop(&mut model);
}

type Graph = mf::Graph<Mutex<Box<GuiModule>>>;
type Interface = mf::Interface<Mutex<Box<GuiModule>>>;

/// Model holds info about GUI state
/// and program state
struct Model {
    time: f32,
    graph: Arc<Graph>,
    window_size: [f32; 2],
    mouse_pos: [f32; 2],
    node_z: Vec<mf::NodeId>,
}

impl Model {
    fn new() -> Model {
        Model {
            graph: Graph::new(),
            time: 0.0,
            window_size: [0.0, 0.0],
            mouse_pos: [0.0, 0.0],
            node_z: Vec::new(),
        }
    }

    fn update_depth(&mut self) {
        use std::collections::HashSet;
        let graph_nodes = self.graph.node_map();
        let mut seen = HashSet::new();
        self.node_z.drain_filter(|node| {
            seen.insert(*node);
            !graph_nodes.contains_key(node)
        });
        for (id, _) in &graph_nodes {
            if !seen.contains(id) {
                self.node_z.push(*id);
            }
        }
    }
    fn update(&mut self) {
        self.update_depth();
    }

    fn handle(&mut self, event: &glutin::Event) {
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
                        ElementState::Pressed => {
                            let graph_nodes = self.graph.node_map();
                            let mut hit = false;
                            let mut idx = self.node_z.len();
                            for id in self.node_z.iter().rev() {
                                idx -= 1;
                                if let Some(node) = graph_nodes.get(id) {
                                    let mut module = node.module().lock().unwrap();
                                    let rect = *module.window_rect();
                                    if point_in_rect(self.mouse_pos, &rect) {
                                        module.set_drag(Some([
                                            self.mouse_pos[0] - rect.translate[0],
                                            self.mouse_pos[1] - rect.translate[1],
                                        ]));
                                        hit = true;
                                        break;
                                    }
                                }
                            };
                            if hit {
                                let removed = self.node_z.remove(idx);
                                self.node_z.push(removed);
                            }
                        }
                        ElementState::Released => {
                            let graph_nodes = self.graph.node_map();
                            for id in self.node_z.iter().rev() {
                                if let Some(node) = graph_nodes.get(id) {
                                    let mut module = node.module().lock().unwrap();
                                    module.set_drag(None);
                                }
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

fn main_loop(model: &mut Model) {
    // init window
    let mut events_loop = EventsLoop::new();
    let context = ContextBuilder::new().with_gl_profile(glutin::GlProfile::Core);
    let builder = WindowBuilder::new().with_title(String::from("flow-synth"));
    let (window, mut device, mut factory, mut main_color, mut main_depth) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop);

    let mut target = Target {
        color: main_color,
        depth: main_depth,
    };

    // init rendering pipeline
    let mut ctx = RenderContext::new(factory.clone());

    let mod_ctx = ctx.clone();
    model.graph.add_node(&mf::MetaModule::new(
        "foo",
        Arc::new(move |ifc| {
            Mutex::new(Box::new(GuiModuleWrapper::new(
                TestModule::new(ifc),
                mod_ctx.clone(),
            )))
        }),
    ));
    let mod_ctx = ctx.clone();
    model.graph.add_node(&mf::MetaModule::new(
        "bar",
        Arc::new(move |ifc| {
            let mut w = GuiModuleWrapper::new(TestModule::new(ifc), mod_ctx.clone());
            w.window_rect.translate[0] = 0.5;
            Mutex::new(Box::new(w))
        }),
    ));

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
        model.update();
        events_loop.poll_events(|event| {
            model.handle(&event);
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
                        gfx_glutin::update_views(&window, &mut target.color, &mut target.depth);
                    }
                    _ => (),
                },
                _ => (),
            }
        });

        ctx.begin_frame(&target);

        let mut z_idx = 0;
        let graph_nodes = model.graph.node_map();
        for id in &model.node_z {
            match graph_nodes.get(id) {
                Some(node) => {
                    let mut module = node.module().lock().unwrap();
                    module.render(&mut device, &mut ctx, -z_idx);
                    z_idx += 1;
                }
                None => (), // node was removed between call to model.update() and here. safe to ignore
            }
        }

        // debug text
        ctx.draw_text(
            &format!("FPS={} Time={}", frames.len(), model.time),
            [0.5, 0.5],
            [1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device, &target);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

#[derive(Clone)]
struct RectRenderer {
    factory: gl::Factory,
    pso: PipelineState<gl::Resources, rect_pipe::Meta>,
    slice: Slice<gl::Resources>,
    vertex_buffer: Buffer<gl::Resources, Vertex>,
    rects: Vec<ColoredRect>,
}
impl RectRenderer {
    fn new(mut factory: gl::Factory) -> RectRenderer {
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
                rect_pipe::new(),
            )
            .unwrap();
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
        RectRenderer {
            factory,
            pso,
            slice,
            vertex_buffer,
            rects: Vec::new(),
        }
    }
    fn push(&mut self, rect: ColoredRect) {
        self.rects.push(rect);
    }
    fn draw(
        &mut self,
        encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>,
        target: &Target,
    ) {
        let instance_buffer = self.factory
            .create_buffer_immutable(&self.rects, Role::Vertex, Bind::empty())
            .unwrap();
        let data = rect_pipe::Data {
            aspect_ratio: aspect_ratio(target_dimensions(target)),
            vertices: self.vertex_buffer.clone(),
            instances: instance_buffer,
            out: target.color.clone(),
            depth: target.depth.clone(),
        };
        self.slice.instances = Some((self.rects.len() as u32, 0));
        encoder.draw(&self.slice, &self.pso, &data);
        self.rects.clear();
    }
}

#[derive(Clone)]
struct TexturedRectRenderer {
    factory: gl::Factory,
    pso: PipelineState<gl::Resources, textured_rect_pipe::Meta>,
    sampler: Sampler<gl::Resources>,
    rects: Vec<(Rect, ShaderResourceView<gl::Resources, [f32; 4]>)>,
}

impl TexturedRectRenderer {
    fn new(mut factory: gl::Factory) -> TexturedRectRenderer {
        let pso = factory
            .create_pipeline_simple(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/textured_rect_150.glslv"
                )),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/shaders/textured_rect_150.glslf"
                )),
                textured_rect_pipe::new(),
            )
            .unwrap();
        let sampler = factory.create_sampler_linear();
        TexturedRectRenderer {
            factory,
            pso,
            sampler,
            rects: Vec::new(),
        }
    }
    fn push(&mut self, rect: Rect, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.rects.push((rect, texture));
    }
    fn draw(
        &mut self,
        encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>,
        target: &Target,
    ) {
        for (rect, texture) in self.rects.drain(..) {
            let vertices = [
                TexturedVertex {
                    translate: [rect.translate[0], rect.translate[1], rect.translate[2]],
                    tex_coord: [0.0, 0.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0],
                        rect.translate[1] + rect.scale[1],
                        rect.translate[2],
                    ],
                    tex_coord: [0.0, 1.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0] + rect.scale[0],
                        rect.translate[1] + rect.scale[1],
                        rect.translate[2],
                    ],
                    tex_coord: [1.0, 1.0],
                },
                TexturedVertex {
                    translate: [
                        rect.translate[0] + rect.scale[0],
                        rect.translate[1],
                        rect.translate[2],
                    ],
                    tex_coord: [1.0, 0.0],
                },
            ];
            let (vertex_buffer, slice) = self.factory
                .create_vertex_buffer_with_slice(&vertices, &RECT_IDX[..]);
            let data = textured_rect_pipe::Data {
                texture: (texture, self.sampler.clone()),
                vertices: vertex_buffer,
                out: target.color.clone(),
                depth: target.depth.clone(),
            };
            encoder.draw(&slice, &self.pso, &data);
        }
    }
}

#[derive(Clone)]
struct TextRenderer {
    renderer: Arc<Mutex<gfx_text::Renderer<gl::Resources, gl::Factory>>>,
    texts: Vec<(String, [f32; 2], [f32; 3])>,
}
impl TextRenderer {
    fn new(mut factory: gl::Factory) -> TextRenderer {
        let renderer = Arc::new(Mutex::new(gfx_text::new(factory.clone()).unwrap()));
        TextRenderer {
            renderer,
            texts: Vec::new(),
        }
    }
    fn push(&mut self, text: &str, pos: [f32; 2], color: [f32; 3]) {
        self.texts.push((text.into(), pos, color));
    }
    fn draw(
        &mut self,
        encoder: &mut Encoder<gl::Resources, gl::CommandBuffer>,
        target: &Target,
    ) {
        let mut renderer = self.renderer.lock().unwrap();
        for (text, pos, color) in self.texts.drain(..) {
            renderer.add(
                &text,
                [pos[0] as i32, pos[1] as i32],
                [color[0], color[1], color[2], 1.0],
            );
        }
        renderer.draw(encoder, &target.color);
    }
}

struct RenderContext {
    factory: gl::Factory,
    encoder: Encoder<gl::Resources, gl::CommandBuffer>,
    rects: RectRenderer,
    textured_rects: TexturedRectRenderer,
    texts: TextRenderer,
}
impl RenderContext {
    fn new(mut factory: gl::Factory) -> RenderContext {
        let encoder = factory.create_command_buffer().into();
        let rects = RectRenderer::new(factory.clone());
        let textured_rects = TexturedRectRenderer::new(factory.clone());
        let texts = TextRenderer::new(factory.clone());

        RenderContext {
            factory,
            encoder,
            rects,
            textured_rects,
            texts,
        }
    }
    pub fn begin_frame(&mut self, target: &Target) {
        self.encoder.clear(&target.color, BLACK);
        self.encoder.clear_depth(&target.depth, 1.0);
    }
    pub fn draw_text(&mut self, text: &str, pos: [f32; 2], color: [f32; 3]) {
        self.texts.push(text, pos, color);
    }
    pub fn draw_rect(&mut self, rect: ColoredRect) {
        self.rects.push(rect);
    }
    pub fn draw_textured_rect(&mut self, rect: Rect, texture: ShaderResourceView<gl::Resources, [f32; 4]>) {
        self.textured_rects.push(rect, texture);
    }
    pub fn factory(&self) -> &gl::Factory {
        &self.factory
    }
    pub fn end_frame(
        &mut self,
        device: &mut gl::Device,
        target: &Target,
    ) {
        self.rects.draw(&mut self.encoder, target);
        self.textured_rects.draw(&mut self.encoder, target);
        self.texts.draw(&mut self.encoder, target);
        self.encoder.flush(device);
    }
}

impl Clone for RenderContext {
    fn clone(&self) -> RenderContext {
        let mut factory = self.factory.clone();
        let encoder = factory.create_command_buffer().into();
        RenderContext {
            factory: factory,
            encoder: encoder,
            rects: self.rects.clone(),
            textured_rects: self.textured_rects.clone(),
            texts: self.texts.clone(),
        }
    }
}

trait Module {
    fn start(&mut self);
    fn title(&self) -> String;
}
trait GuiModule: Module {
    fn update(&mut self, model: &Model, &glutin::Event);
    fn render(&mut self, &mut gl::Device, &mut RenderContext, i32);
    fn window_rect(&self) -> &Rect;
    fn set_drag(&mut self, drag: Option<[f32; 2]>);
}
struct GuiModuleWrapper<Module> {
    module: Module,

    window_rect: Rect,
    drag: Option<[f32; 2]>,

    internal_ctx: RenderContext,
    module_target_texture: Texture<gl::Resources, gfx::format::R8_G8_B8_A8>,
    module_depth_texture: Texture<gl::Resources, gfx::format::D24_S8>,
    module_target_resource: ShaderResourceView<gl::Resources, [f32; 4]>,
    module_target: Target,
}

impl<Module> GuiModuleWrapper<Module> {
    fn new(module: Module, ctx: RenderContext) -> GuiModuleWrapper<Module> {
        let mut factory = ctx.factory.clone();
        let module_target_texture = factory
            .create_texture(
                texture::Kind::D2(128, 128, texture::AaMode::Single),
                1, //levels
                gfx::RENDER_TARGET | gfx::SHADER_RESOURCE,
                Usage::Data,
                Some(gfx::format::ChannelType::Unorm),
            )
            .unwrap();
        let module_color_target = factory
            .view_texture_as_render_target(
                &module_target_texture,
                0,    //level
                None, //layer
            )
            .unwrap();
        let module_target_resource = factory
            .view_texture_as_shader_resource::<gfx::format::Rgba8>(
                &module_target_texture,
                (0, 0), // levels
                gfx::format::Swizzle::new(),
            )
            .unwrap();

        let module_depth_texture = factory
            .create_texture(
                texture::Kind::D2(128, 128, texture::AaMode::Single),
                1, //levels
                gfx::DEPTH_STENCIL,
                Usage::Data,
                Some(gfx::format::ChannelType::Unorm),
            )
            .unwrap();
        let module_depth_target = factory
            .view_texture_as_depth_stencil(
                &module_depth_texture,
                0,    //level
                None, //layer
                texture::DepthStencilFlags::empty(),
            )
            .unwrap();

        let module_target = Target {
            color: module_color_target,
            depth: module_depth_target,
        };

        GuiModuleWrapper {
            module,
            window_rect: Rect {
                translate: [0.0, 0.0, 0.0],
                scale: [0.4, 0.4],
            },
            drag: None,
            internal_ctx: ctx,
            module_target_resource,
            module_target_texture,
            module_depth_texture,
            module_target,
        }
    }
}

impl<T> GuiModule for GuiModuleWrapper<T>
where
    T: Module,
{
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, z_idx: i32) {
        self.internal_ctx.begin_frame(&self.module_target);

        self.internal_ctx.draw_rect(ColoredRect {
            translate: [-1.0, -1.0, 0.0],
            scale: [2.0, 2.0],
            color: [1.0, 1.0, 1.0],
        });
        let title = &self.title();
        self.internal_ctx
            .draw_text(title, [0.0, 0.0], [0.0, 0.0, 0.0]);

        self.internal_ctx.end_frame(device, &self.module_target);

        self.window_rect.translate[2] = z_idx as f32;
        ctx.draw_textured_rect(self.window_rect, self.module_target_resource.clone());
    }
    fn update(&mut self, model: &Model, event: &glutin::Event) {
        if let Some(drag) = self.drag {
            self.window_rect.translate = [
                -drag[0] + model.mouse_pos[0],
                -drag[1] + model.mouse_pos[1],
                0.0,
            ];
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
                    _ => (),
                },
                _ => (),
            },
            _ => (),
        }
    }
    fn window_rect(&self) -> &Rect {
        &self.window_rect
    }
    fn set_drag(&mut self, drag: Option<[f32; 2]>) {
        self.drag = drag;
    }
}
impl<T> Module for GuiModuleWrapper<T>
where
    T: Module,
{
    fn start(&mut self) {
        T::start(&mut self.module);
    }
    fn title(&self) -> String {
        T::title(&self.module)
    }
}

struct TestModule {
    ifc: Arc<Interface>,
}
impl TestModule {
    fn new(ifc: Arc<Interface>) -> TestModule {
        TestModule { ifc }
    }
}
impl Module for TestModule {
    fn start(&mut self) {
        println!("start!!");
    }
    fn title(&self) -> String {
        self.ifc.meta().name.to_string()
    }
}

fn point_in_rect(pos: [f32; 2], rect: &Rect) -> bool {
    pos[0] >= rect.translate[0] && pos[0] <= rect.translate[0] + rect.scale[0] && pos[1] >= rect.translate[1]
        && pos[1] <= rect.translate[1] + rect.scale[1]
}
fn pixels_to_coords(size: [f32; 2], pix: [f32; 2]) -> [f32; 2] {
    let aspect = size[0] / size[1];
    [
        aspect * (pix[0] / size[0] * 2.0 - 1.0),
        pix[1] / size[1] * -2.0 + 1.0,
    ]
}
fn coords_to_pixels(size: [f32; 2], coord: [f32; 2]) -> [f32; 2] {
    let aspect = size[0] / size[1];
    [
        (coord[0] / aspect * 0.5 + 0.5) * size[0],
        (-coord[1] * 0.5 + 0.5) * size[1],
    ]
}
fn target_dimensions(target: &Target) -> [f32; 2] {
    let dims = target.color.get_dimensions();
    [dims.0 as f32, dims.1 as f32]
}
fn aspect_ratio(size: [f32; 2]) -> f32 {
    size[0] / size[1]
}
