mod render;
mod menu;
mod button;
mod geom;
mod component;
mod event;

use self::render::*;
use super::module::*;
use self::menu::{Menu, MenuUpdate, MenuView};
use self::button::*;
use self::geom::*;
use self::component::*;
use self::event::*;

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
                        let name = path[0].as_ref();
                        let module = self.module_types.iter().find(|ty| ty.name == name).unwrap();
                        self.new_module(module, Rect2::new(self.mouse_pos, Pt2::fill(256.0)));
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
        if ButtonState::Pressed == state && MouseButton::Left == button {
            self.context_menu = None;
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
                .map(|ty| menu::item(&ty.name))
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
    while running {
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

const TITLE_BAR_HEIGHT: f32 = 24.0;
const BORDER_SIZE: f32 = 1.0;

struct GuiModuleWrapper<T: Module> {
    module: T,

    target: TextureTarget,

    delete_button: Button,
    bounds: Box3,
    drag: Option<Pt2>,
    dirty: bool,
}

impl<T: Module> GuiModuleWrapper<T> {
    fn new(module: T, ctx: RenderContext, bounds: Box3) -> GuiModuleWrapper<T> {
        let target = TextureTarget::new(ctx.clone(), bounds.size.drop_z());

        GuiModuleWrapper {
            module,
            target,
            delete_button: Button::new(
                ctx,
                "X".into(),
                Box3 {
                    pos: Pt3::new(
                        bounds.size.x - TITLE_BAR_HEIGHT - BORDER_SIZE,
                        BORDER_SIZE,
                        0.0,
                    ),
                    size: Pt3::new(TITLE_BAR_HEIGHT, TITLE_BAR_HEIGHT, 0.0),
                },
            ),
            bounds,
            drag: None,
            dirty: true,
        }
    }
    fn render_self(&mut self) {
        let title = &self.title();
        let ctx = self.target.ctx();
        // borders
        ctx.draw_rect(
            Rect3::new(Pt3::new(0.0, 0.0, 1.0), self.bounds.size.drop_z()),
            [1.0, 1.0, 1.0],
        );
        // background
        ctx.draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE + TITLE_BAR_HEIGHT, 0.9),
                self.bounds.size.drop_z() - Pt2::new(BORDER_SIZE * 2.0, BORDER_SIZE * 2.0 + TITLE_BAR_HEIGHT),
            ),
            [0.1, 0.1, 0.1],
        );
        // title bar
        ctx.draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE, 0.9),
                Pt2::new(self.bounds.size.x - BORDER_SIZE * 2.0, TITLE_BAR_HEIGHT),
            ),
            [0.0, 0.0, 0.0],
        );
        ctx.draw_text(title, Pt3::new(4.0, 4.0, 0.8), [1.0, 1.0, 1.0]);
    }
}

enum GuiModuleUpdate {
    Unchanged,
    Closed,
}

impl<T> GuiComponent<GuiModuleUpdate> for GuiModuleWrapper<T>
where
    T: Module,
{
    fn bounds(&self) -> Box3 {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds = bounds;
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds.flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        if self.dirty {
            self.target.begin_frame();
            self.render_self();
            self.delete_button.render(device, self.target.ctx());
            self.target.end_frame(device);
            self.dirty = false;
        }

        ctx.draw_textured_rect(self.bounds.flatten(), self.target.shader_resource().clone());
    }
    fn handle(&mut self, event: &Event) -> GuiModuleUpdate {
        let origin = self.bounds.pos.drop_z();
        match event.data {
            EventData::MouseMove(pos) => {
                self.dirty |=
                    ButtonUpdate::NeedRender == self.delete_button.handle(&event.translate(-origin));
                if let Some(drag) = self.drag {
                    self.bounds.pos.x = -drag.x + pos.x;
                    self.bounds.pos.y = -drag.y + pos.y;
                }
                GuiModuleUpdate::Unchanged
            }
            EventData::Click(pos, button, state) => {
                if self.delete_button.intersect(pos - origin) {
                    if ButtonUpdate::Clicked == self.delete_button.handle(&event.translate(-origin)) {
                        GuiModuleUpdate::Closed
                    } else {
                        GuiModuleUpdate::Unchanged
                    }
                } else {
                    match button {
                        MouseButton::Left => match state {
                            ButtonState::Pressed => {
                                let mut title_rect = self.bounds.flatten().drop_z();
                                title_rect.size = Pt2::new(title_rect.size.x, TITLE_BAR_HEIGHT + BORDER_SIZE);
                                if title_rect.intersect(pos) {
                                    self.drag = Some(pos - origin);
                                }
                            }
                            ButtonState::Released => {
                                self.drag = None;
                            }
                        },
                        _ => {}
                    }
                    GuiModuleUpdate::Unchanged
                }
            }
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
    fn title(&self) -> String {
        T::title(&self.module)
    }
}
