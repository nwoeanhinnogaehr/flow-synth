//! Root component that holds the application

use super::geom::*;
use super::render::*;
use super::super::module::debug::*;
use super::component::*;
use super::event::*;
use super::module_gui::*;
use super::menu::*;
use super::connect::*;

use modular_flow as mf;
use gfx_device_gl as gl;

use std::sync::Arc;
use std::rc::Rc;
use std::cmp::Ordering;

pub struct Root {
    graph: Arc<mf::Graph>,
    bounds: Box3,

    ctx: RenderContext,
    modules: Vec<Box<GuiModule>>,
    module_types: Vec<Box<GuiModuleFactory>>,
    context_menu: Option<MenuView>,
    jack_ctx: Rc<JackContext<Arc<mf::Port>>>,
}

impl Root {
    pub fn new(ctx: RenderContext, bounds: Box3) -> Root {
        Root {
            graph: mf::Graph::new(),
            bounds,
            modules: Vec::new(),
            module_types: load_metamodules(),
            context_menu: None,
            jack_ctx: JackContext::new(bounds),

            ctx,
        }
    }

    fn new_module(&mut self, name: &str, rect: Rect2) -> Result<mf::NodeId, ()> {
        // dummy z, overwritten by move_to_front
        if let Some(factory) = self.module_types.iter_mut().find(|ty| ty.name() == name) {
            let bounds = Box3::new(rect.pos.with_z(0.0), rect.size.with_z(0.0));
            let module = factory.new(GuiModuleConfig {
                bounds,
                jack_ctx: Rc::clone(&self.jack_ctx),
                graph: Arc::clone(&self.graph),
                ctx: self.ctx.clone(),
            });
            let id = module.node().id();
            self.modules.push(module);
            self.move_to_front(id);
            Ok(id)
        } else {
            Err(())
        }
    }

    fn open_new_module_menu(&mut self, pos: Pt2) {
        self.context_menu = Some(MenuView::new(
            self.ctx.clone(),
            Box3::new(
                pos.with_z(0.0),
                (self.bounds.size.drop_z() - pos).with_z(0.0),
            ),
            Menu::new(&self.module_types
                .iter()
                .map(|ty| item(&ty.name()))
                .collect::<Vec<_>>()),
        ));
    }

    fn compare_node_z(a: &Box<GuiModule>, b: &Box<GuiModule>) -> Ordering {
        let a_z = a.bounds().pos.z;
        let b_z = b.bounds().pos.z;
        a_z.partial_cmp(&b_z).unwrap()
    }

    fn move_to_front(&mut self, id: mf::NodeId) {
        self.modules.sort_by(|a, b| {
            // force given id to front
            if a.node().id() == id {
                Ordering::Less
            } else if b.node().id() == id {
                Ordering::Greater
            } else {
                Self::compare_node_z(a, b)
            }
        });
        let max = self.modules.len() as f32;
        for (idx, module) in self.modules.iter_mut().enumerate() {
            let mut bounds = module.bounds();
            bounds.pos.z = idx as f32 / max;
            bounds.size.z = 1.0 / max;
            module.set_bounds(bounds);
        }
    }
}

impl GuiComponent for Root {
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds = bounds;
    }
    fn bounds(&self) -> Box3 {
        self.bounds
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds.flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        // render nodes
        for module in &mut self.modules {
            module.render(device, ctx);
        }

        // render global widgets
        if let Some(menu) = self.context_menu.as_mut() {
            menu.render(device, ctx);
        }

        // render wires
        self.jack_ctx.render(device, ctx);
    }
    fn handle(&mut self, event: &Event) {
        match event.data {
            EventData::MouseMove(pos) | EventData::Click(pos, _, _) => {
                // march from front to back, if we hit something set this flag so that we only send
                // one event with focus
                let mut hit = false;

                // intersect menu
                if let Some(menu) = self.context_menu.as_mut() {
                    if menu.intersect(pos) {
                        hit = true;
                        let status = menu.handle(&event.with_focus(true));
                        match status {
                            MenuUpdate::Select(path) => {
                                let name: &str = path[0].as_ref();
                                self.new_module(name, Rect2::new(pos, 256.0.into()))
                                    .unwrap();
                                self.context_menu = None;
                            }
                            _ => (),
                        }
                    } else {
                        // assume unfocused events are boring
                        menu.handle(&event.with_focus(false));
                    }
                }

                // intersect nodes
                let mut hit_module = None;
                for (idx, module) in self.modules.iter_mut().enumerate() {
                    if !hit && module.intersect(pos) {
                        hit = true;
                        hit_module = Some(idx);
                    } else {
                        // assume unfocused events are boring
                        module.handle(&event.with_focus(false));
                    }
                }
                if let Some(idx) = hit_module {
                    let status = self.modules[idx].handle(&event.with_focus(true));
                    if let EventData::Click(_, _, _) = event.data {
                        match status {
                            GuiModuleUpdate::Closed => {
                                self.modules.remove(idx);
                            }
                            _ => {
                                let id = self.modules[idx].node().id();
                                self.move_to_front(id);
                            }
                        }
                    }
                }

                if let EventData::Click(_, button, state) = event.data {
                    // right click - open menu
                    if ButtonState::Pressed == state && MouseButton::Right == button {
                        self.open_new_module_menu(pos);
                    }
                    // left click - abort menu
                    if let Some(menu) = self.context_menu.as_mut() {
                        if !menu.intersect(pos) && ButtonState::Pressed == state
                            && MouseButton::Left == button
                        {
                            self.context_menu = None;
                        }
                    }
                }
            }
        }
    }
}

fn load_metamodules() -> Vec<Box<GuiModuleFactory>> {
    vec![
        Box::new(BasicGuiModuleFactory::<Printer<i32>>::new()),
        Box::new(BasicGuiModuleFactory::<Counter<i32>>::new()),
    ]
}
