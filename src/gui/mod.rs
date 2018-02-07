mod render;
mod menu;
mod button;
mod geom;
mod component;
mod event;
mod module_gui;

use self::render::*;
use super::module::*;
use self::menu::{Menu, MenuUpdate, MenuView};
use self::geom::*;
use self::component::*;
use self::event::*;
use self::module_gui::*;

use glutin::{self, ContextBuilder, EventsLoop, GlContext, WindowBuilder};
use modular_flow as mf;

use std::time::Instant;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::cmp::Ordering;

use gfx::Device;
use gfx_window_glutin as gfx_glutin;
use gfx_device_gl as gl;

type OwnedModule = Mutex<Box<GuiComponent<GuiModuleUpdate>>>;
type Graph = mf::Graph<OwnedModule>;
type Node = mf::Node<OwnedModule>;

/// Model holds info about GUI state
/// and program state
pub struct Model {
    ctx: RenderContext,
    time: f32,
    graph: Arc<Graph>,
    window_size: Pt2,
    mouse_pos: Pt2,
    module_types: Vec<mf::MetaModule<OwnedModule>>,
    context_menu: Option<MenuView>,
}

impl Model {
    fn new(ctx: RenderContext) -> Model {
        Model {
            graph: Graph::new(),
            time: 0.0,
            window_size: Pt2::fill(0.0),
            mouse_pos: Pt2::fill(0.0),
            module_types: load_metamodules(ctx.clone()),
            context_menu: None,
            ctx,
        }
    }

    fn handle_mouse_input(&mut self, state: ButtonState, button: MouseButton) {
        let event = Event {
            time: self.time,
            data: EventData::Click(self.mouse_pos, button.into(), state.into()),
        };
        // intersect menu
        if let Some(menu) = self.context_menu.as_mut() {
            if menu.intersect(self.mouse_pos) {
                let status = menu.handle(&event);
                match status {
                    MenuUpdate::Select(path) => {
                        let name: &str = path[0].as_ref();
                        if let Some(module) = self.module_types.iter().find(|ty| ty.name() == name) {
                            self.new_module(module, Rect2::new(self.mouse_pos, Pt2::fill(256.0)));
                        } else {
                            println!("Couldn't find module {}", name);
                        }
                        self.context_menu = None;
                    }
                    _ => (),
                }
                return;
            }
        }

        // intersect nodes
        let mut nodes = self.graph.nodes();
        nodes.sort_by(Self::compare_node_z);
        for node in &nodes {
            let mut module = node.module().lock().unwrap();
            if module.intersect(self.mouse_pos) {
                let status = module.handle(&event);
                drop(module);
                match status {
                    GuiModuleUpdate::Closed => {
                        self.graph.remove_node(node.id()).unwrap();
                    }
                    _ => {}
                }
                self.move_to_front(node.id());
                break;
            }
        }

        // right click - open menu
        if ButtonState::Pressed == state && MouseButton::Right == button {
            self.open_new_module_menu();
        }
        // left click - abort menu
        if let Some(menu) = self.context_menu.as_mut() {
            if !menu.intersect(self.mouse_pos) && ButtonState::Pressed == state && MouseButton::Left == button
            {
                self.context_menu = None;
            }
        }
    }

    fn compare_node_z(a: &Arc<Node>, b: &Arc<Node>) -> Ordering {
        let a_z = a.module().lock().unwrap().bounds().pos.z;
        let b_z = b.module().lock().unwrap().bounds().pos.z;
        a_z.partial_cmp(&b_z).unwrap()
    }

    fn move_to_front(&self, id: mf::NodeId) {
        let mut nodes = self.graph.nodes();
        nodes.sort_by(|a, b| {
            // force given id to front
            if a.id() == id {
                Ordering::Less
            } else if b.id() == id {
                Ordering::Greater
            } else {
                Self::compare_node_z(a, b)
            }
        });
        let max = nodes.len() as f32;
        for (idx, node) in nodes.iter().enumerate() {
            let mut module = node.module().lock().unwrap();
            let mut bounds = module.bounds();
            bounds.pos.z = idx as f32 / max;
            bounds.size.z = 1.0 / max;
            module.set_bounds(bounds);
        }
    }

    fn new_module(&self, meta: &mf::MetaModule<OwnedModule>, rect: Rect2) {
        // dummy z, overwritten by move_to_front
        let bounds = Box3::new(rect.pos.with_z(0.0), rect.size.with_z(0.0));
        let node = self.graph.add_node(meta, bounds);
        self.move_to_front(node.id());
    }

    fn open_new_module_menu(&mut self) {
        self.context_menu = Some(MenuView::new(
            self.ctx.clone(),
            Box3::new(
                self.mouse_pos.with_z(0.0),
                (self.window_size - self.mouse_pos).with_z(0.0),
            ),
            Menu::new(&self.module_types
                .iter()
                .map(|ty| menu::item(&ty.name()))
                .collect::<Vec<_>>()),
        ));
    }

    fn generate_event(&mut self, data: EventData) {
        let event = Event {
            time: self.time,
            data,
        };
        for node in self.graph.nodes() {
            let mut module = node.module().lock().unwrap();
            module.handle(&event);
        }
        if let Some(menu) = self.context_menu.as_mut() {
            menu.handle(&event);
        }
    }

    fn handle(&mut self, event: &glutin::Event) {
        use glutin::WindowEvent::*;
        //println!("{:?}", event);
        match event {
            glutin::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                Resized(w, h) => {
                    self.window_size = Pt2::new(*w as f32, *h as f32);
                }
                CursorMoved {
                    device_id: _,
                    position,
                    modifiers: _,
                } => {
                    self.mouse_pos = Pt2::new((position.0 as f32).floor(), (position.1 as f32).floor());
                    self.generate_event(EventData::MouseMove(self.mouse_pos));
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                    modifiers: _,
                } => {
                    self.handle_mouse_input(state.into(), button.into());
                }
                _ => (),
            },
            _ => (),
        }
    }

    fn render(&mut self, ctx: &mut RenderContext, device: &mut gl::Device) {
        // render nodes
        let graph_nodes = self.graph.node_map();
        for (id, node) in &graph_nodes {
            let mut module = node.module().lock().unwrap();
            module.render(device, ctx);
        }

        // render global widgets
        if let Some(menu) = self.context_menu.as_mut() {
            menu.render(device, ctx);
        }
    }
}

fn load_metamodules(ctx: RenderContext) -> Vec<mf::MetaModule<OwnedModule>> {
    let mut modules = Vec::new();
    let mod_ctx = ctx;
    let test_module = mf::MetaModule::new(
        "TestModule",
        Arc::new(move |ifc, bounds| {
            Mutex::new(Box::new(GuiModuleWrapper::new(
                TestModule::new(ifc),
                mod_ctx.clone(),
                bounds,
            )) as Box<GuiComponent<GuiModuleUpdate>>)
        }),
    );
    modules.push(test_module);
    modules
}

pub fn gui_main() {
    // init window
    let mut events_loop = EventsLoop::new();
    let context = ContextBuilder::new().with_gl_profile(glutin::GlProfile::Core);
    let builder = WindowBuilder::new().with_title(String::from("flow-synth"));
    let (window, mut device, factory, main_color, main_depth) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop);

    let mut target = Target {
        color: main_color,
        depth: main_depth,
    };

    // init rendering pipeline
    let mut ctx = RenderContext::new(factory.clone());

    let mut model = Model::new(ctx.clone());

    // begin main loop
    let mut running = true;
    let timer = Instant::now();
    let mut frames = VecDeque::new();
    loop {
        // update fps
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

        // handle events
        events_loop.poll_events(|event| {
            model.handle(&event);
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

        if !running {
            break;
        }

        ctx.begin_frame(&target);

        model.render(&mut ctx, &mut device);

        // debug text
        ctx.draw_text(
            &format!("FPS={} Time={}", frames.len(), model.time),
            Pt3::new(0.0, 0.0, 0.0),
            [1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device, &target);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
