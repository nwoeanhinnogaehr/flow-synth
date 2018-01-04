mod render;

use self::render::*;
use super::module::*;

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
use gfx::handle::{Buffer, DepthStencilView, RenderTargetView, Sampler, ShaderResourceView, Texture};
use gfx_window_glutin as gfx_glutin;
use gfx_text;
use gfx_device_gl as gl;

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

    fn handle_mouse_input(&mut self, state: &glutin::ElementState, button: &glutin::MouseButton) {
        use glutin::*;

        let graph_nodes = self.graph.node_map();
        let mut hit = false;
        let mut idx = self.node_z.len();
        for id in self.node_z.iter().rev() {
            idx -= 1;
            if let Some(node) = graph_nodes.get(id) {
                let mut module = node.module().lock().unwrap();
                let rect = *module.window_rect();
                if point_in_rect(self.mouse_pos, &rect) {
                    module.handle_click(self, state, button);
                    hit = true;
                    break;
                }
            }
        }
        if hit {
            let removed = self.node_z.remove(idx);
            self.node_z.push(removed);
        }
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
                    self.mouse_pos = [position.0 as f32, position.1 as f32];
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                } => {
                    self.handle_mouse_input(&state, &button);
                }
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
            w.window_rect.translate[0] = 256.0;
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
        for node in model.graph.nodes() {
            let mut module = node.module().lock().unwrap();
            module.update(&model);
        }
        events_loop.poll_events(|event| {
            model.handle(&event);
            for node in model.graph.nodes() {
                let mut module = node.module().lock().unwrap();
                module.handle(&model, &event);
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
            [0.0, 0.0],
            [1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device, &target);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

trait GuiModule: Module {
    fn render(&mut self, &mut gl::Device, &mut RenderContext, i32);
    fn window_rect(&self) -> &Rect;

    fn update(&mut self, model: &Model);
    fn handle(&mut self, model: &Model, event: &glutin::Event);
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton);
}
struct GuiModuleWrapper<Module> {
    module: Module,

    window_rect: Rect,
    size: [f32; 2],
    drag: Option<[f32; 2]>,

    internal_ctx: RenderContext,
    module_target_texture: Texture<gl::Resources, gfx::format::R8_G8_B8_A8>,
    module_depth_texture: Texture<gl::Resources, gfx::format::D24_S8>,
    module_target_resource: ShaderResourceView<gl::Resources, [f32; 4]>,
    module_target: Target,
}

impl<Module> GuiModuleWrapper<Module> {
    fn new(module: Module, ctx: RenderContext) -> GuiModuleWrapper<Module> {
        let mut factory = ctx.factory().clone();
        let size = [256.0; 2];
        let module_target_texture = factory
            .create_texture(
                texture::Kind::D2(size[0] as u16, size[1] as u16, texture::AaMode::Single),
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
                texture::Kind::D2(size[0] as u16, size[1] as u16, texture::AaMode::Single),
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
                scale: size,
            },
            size,
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
            translate: [0.0, 0.0, 0.0],
            scale: self.size,
            color: [1.0, 1.0, 1.0],
        });
        let title = &self.title();
        self.internal_ctx
            .draw_text(title, [16.0, 16.0], [0.0, 0.0, 0.0]);

        self.internal_ctx.end_frame(device, &self.module_target);

        self.window_rect.translate[2] = z_idx as f32;
        ctx.draw_textured_rect(self.window_rect, self.module_target_resource.clone());
    }
    fn update(&mut self, model: &Model) {
        if let Some(drag) = self.drag {
            self.window_rect.translate = [
                -drag[0] + model.mouse_pos[0],
                -drag[1] + model.mouse_pos[1],
                0.0,
            ];
        }
    }
    fn handle(&mut self, model: &Model, event: &glutin::Event) {
    }
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton) {
        use glutin::*;
        match button {
            MouseButton::Left => {
                match state {
                    ElementState::Pressed => {
                        self.drag = Some([
                            model.mouse_pos[0] - self.window_rect.translate[0],
                            model.mouse_pos[1] - self.window_rect.translate[1],
                        ]);
                    },
                    ElementState::Released => {
                        self.drag = None;
                    }
                }
            }
            _ => {},
        }
    }
    fn window_rect(&self) -> &Rect {
        &self.window_rect
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