mod render;
mod menu;
mod button;
mod geom;
mod component;

use self::render::*;
use super::module::*;
use self::menu::{Menu, MenuManager, MenuUpdate};
use self::button::*;
use self::geom::*;
use self::component::*;

use glutin::{self, ContextBuilder, ElementState, EventsLoop, GlContext, MouseButton, WindowBuilder};
use modular_flow as mf;

use std::sync::mpsc::Receiver;
use std::time::Instant;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use gfx::Device;
use gfx_window_glutin as gfx_glutin;
use gfx_device_gl as gl;

type OwnedModule = Mutex<Box<GuiElement>>;
type Graph = mf::Graph<OwnedModule>;

/// Model holds info about GUI state
/// and program state
pub struct Model {
    time: f32,
    graph: Arc<Graph>,
    window_size: Pt2,
    mouse_pos: Pt2,
    node_z: Vec<mf::NodeId>,
    module_types: Vec<mf::MetaModule<OwnedModule>>,
    menu: Mutex<MenuManager>,
    menu_chan: Option<Receiver<MenuUpdate>>,
}

impl Model {
    fn new(ctx: RenderContext) -> Model {
        Model {
            graph: Graph::new(),
            time: 0.0,
            window_size: Pt2::fill(0.0),
            mouse_pos: Pt2::fill(0.0),
            node_z: Vec::new(),
            module_types: load_metamodules(ctx.clone()),
            menu: Mutex::new(MenuManager::new(ctx)),
            menu_chan: None,
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

    fn handle_menu_updates(&mut self) {
        if self.menu_chan.is_some() {
            let chan = self.menu_chan.take().unwrap();
            match chan.try_recv() {
                Ok(MenuUpdate::Select(path)) => {
                    let name = path[0].as_ref();
                    let module = self.module_types.iter().find(|ty| ty.name == name).unwrap();
                    self.new_module(module, self.mouse_pos);
                }
                Ok(MenuUpdate::Abort) => {}
                _ => {
                    self.menu_chan = Some(chan);
                }
            }
        }
    }

    fn update(&mut self) {
        self.update_depth();
        for node in self.graph.nodes() {
            let mut module = node.module().lock().unwrap();
            module.update(self);
        }
        self.menu.lock().unwrap().update(self);
        self.handle_menu_updates();
    }

    fn handle_mouse_input(&mut self, state: &glutin::ElementState, button: &glutin::MouseButton) {
        // intersect menu
        {
            let mut menu = self.menu.lock().unwrap();
            if menu.intersect(self.mouse_pos) {
                menu.handle_click(self, state, button);
                return;
            }
        }

        let graph_nodes = self.graph.node_map();
        let mut hit = false;
        let mut idx = self.node_z.len();
        for id in self.node_z.iter().rev() {
            idx -= 1;
            if let Some(node) = graph_nodes.get(id) {
                let mut module = node.module().lock().unwrap();
                if module.intersect(self.mouse_pos) {
                    module.handle_click(self, state, button);
                    hit = true;
                    break;
                }
            }
        }
        if hit {
            if ElementState::Pressed == *state {
                let removed = self.node_z.remove(idx);
                self.node_z.push(removed);
            }
        }

        // right click - open menu
        if ElementState::Pressed == *state && MouseButton::Right == *button {
            self.open_new_module_menu();
        }
        // left click - abort menu
        if ElementState::Pressed == *state && MouseButton::Left == *button {
            self.menu.lock().unwrap().abort();
        }
    }

    fn new_module(&self, meta: &mf::MetaModule<OwnedModule>, pos: Pt2) {
        let node = self.graph.add_node(meta);
        let mut module = node.module().lock().unwrap();
        module.set_pos(pos);
    }

    fn open_new_module_menu(&mut self) {
        self.menu_chan = Some(
            self.menu.lock().unwrap().open(
                Menu::new(&self.module_types
                    .iter()
                    .map(|ty| menu::item(&ty.name))
                    .collect::<Vec<_>>()),
                self.mouse_pos,
            ),
        );
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
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                    modifiers: _,
                } => {
                    self.handle_mouse_input(&state, &button);
                }
                _ => (),
            },
            _ => (),
        }
    }
}

fn load_metamodules(ctx: RenderContext) -> Vec<mf::MetaModule<OwnedModule>> {
    let mut modules = Vec::new();
    let mod_ctx = ctx;
    let test_module = mf::MetaModule::new(
        "TestModule",
        Arc::new(move |ifc| {
            Mutex::new(
                Box::new(GuiModuleWrapper::new(TestModule::new(ifc), mod_ctx.clone())) as Box<GuiElement>,
            )
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

        // update model
        model.update();

        ctx.begin_frame(&target);

        // render nodes
        let mut z_idx = 0;
        let graph_nodes = model.graph.node_map();
        for id in &model.node_z {
            match graph_nodes.get(id) {
                Some(node) => {
                    let mut module = node.module().lock().unwrap();
                    module.render(
                        &mut device,
                        &mut ctx,
                        1.0 - z_idx as f32 / graph_nodes.len() as f32,
                    );
                    z_idx += 1;
                }
                None => (), // node was removed between call to model.update() and here. safe to ignore
            }
        }

        // render global widgets
        model
            .menu
            .lock()
            .unwrap()
            .render(&mut device, &mut ctx, 0.0);

        // debug text
        ctx.draw_text(
            &format!("FPS={} Time={}", frames.len(), model.time),
            Pt2::new(0.0, 0.0),
            [1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device, &target);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}

const TITLE_BAR_HEIGHT: f32 = 24.0;
const BORDER_SIZE: f32 = 1.0;

trait GuiElement {
    fn set_pos(&mut self, pos: Pt2);
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32);
    fn intersect(&self, point: Pt2) -> bool;
    fn update(&mut self, model: &Model);
    fn handle(&mut self, model: &Model, event: &glutin::Event);
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton);
}
struct GuiModuleWrapper<T: Module> {
    module: T,

    target: TextureTarget,

    delete_button: Button,
    window_rect: Rect2,
    size: Pt2,
    drag: Option<Pt2>,
    dirty: bool,
}

impl<T: Module> GuiModuleWrapper<T> {
    fn new(module: T, ctx: RenderContext) -> GuiModuleWrapper<T> {
        let size = Pt2::fill(256.0);
        let target = TextureTarget::new(ctx.clone(), size);

        GuiModuleWrapper {
            module,
            target,
            delete_button: Button::new(
                ctx,
                "X".into(),
                Rect2 {
                    pos: Pt2::new(size.x - TITLE_BAR_HEIGHT - BORDER_SIZE, BORDER_SIZE),
                    size: Pt2::fill(TITLE_BAR_HEIGHT),
                },
            ),
            window_rect: Rect2 {
                pos: Pt2::fill(0.0),
                size,
            },
            size,
            drag: None,
            dirty: true,
        }
    }
    fn render_self(&mut self) {
        let title = &self.title();
        let ctx = self.target.ctx();
        // borders
        ctx.draw_rect(Rect3::new(Pt3::fill(0.0), self.size), [1.0, 1.0, 1.0]);
        // background
        ctx.draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE + TITLE_BAR_HEIGHT, 0.0),
                self.size - Pt2::new(BORDER_SIZE * 2.0, BORDER_SIZE * 2.0 + TITLE_BAR_HEIGHT),
            ),
            [0.1, 0.1, 0.1],
        );
        // title bar
        ctx.draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE, 0.0),
                Pt2::new(self.size.x - BORDER_SIZE * 2.0, TITLE_BAR_HEIGHT),
            ),
            [0.0, 0.0, 0.0],
        );
        ctx.draw_text(title, Pt2::new(4.0, 4.0), [1.0, 1.0, 1.0]);
    }
}

impl<T> GuiElement for GuiModuleWrapper<T>
where
    T: Module,
{
    fn set_pos(&mut self, pos: Pt2) {
        self.window_rect.pos = pos;
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, z_idx: f32) {
        if self.dirty {
            self.target.begin_frame();
            self.render_self();
            self.delete_button
                .render(device, self.target.ctx(), self.window_rect.upgrade(0.0));
            self.target.end_frame(device);
            self.dirty = false;
        }

        ctx.draw_textured_rect(
            self.window_rect.upgrade(z_idx),
            self.target.shader_resource().clone(),
        );
    }
    fn update(&mut self, model: &Model) {
        self.dirty |= ButtonUpdate::NeedRender
            == self.delete_button
                .update(model, self.window_rect.upgrade(0.0));
        if let Some(drag) = self.drag {
            self.window_rect.pos = -drag + model.mouse_pos;
        }
    }
    fn handle(&mut self, model: &Model, event: &glutin::Event) {}
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton) {
        use glutin::*;
        match button {
            MouseButton::Left => match state {
                ElementState::Pressed => {
                    let mut title_rect = self.window_rect;
                    title_rect.size = Pt2::new(title_rect.size.x, TITLE_BAR_HEIGHT + BORDER_SIZE);
                    if title_rect.intersect(model.mouse_pos) {
                        self.drag = Some(model.mouse_pos - self.window_rect.pos);
                    }
                }
                ElementState::Released => {
                    self.drag = None;
                }
            },
            _ => {}
        }
    }
    fn intersect(&self, point: Pt2) -> bool {
        self.window_rect.intersect(point)
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
