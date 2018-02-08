//! Root component that holds the application

use super::geom::*;
use super::render::*;
use super::super::module::*;
use super::component::*;
use super::event::*;
use super::module_gui::*;
use super::menu::*;

use modular_flow as mf;
use gfx_device_gl as gl;

use std::sync::Arc;
use std::cell::{RefCell, RefMut};
use std::cmp::Ordering;

struct OwnedModule {
    value: RefCell<Box<GuiComponent<GuiModuleUpdate>>>,
}
impl OwnedModule {
    pub fn get(&self) -> RefMut<Box<GuiComponent<GuiModuleUpdate>>> {
        self.value.borrow_mut()
    }
}
impl mf::Module for OwnedModule {
    type Arg = Box3;
}
type Graph = mf::Graph<OwnedModule>;
type Node = mf::Node<OwnedModule>;

pub struct Root {
    graph: Arc<Graph>,
    bounds: Box3,

    ctx: RenderContext,
    module_types: Vec<mf::MetaModule<OwnedModule>>,
    context_menu: Option<MenuView>,
}

impl Root {
    pub fn new(ctx: RenderContext, bounds: Box3) -> Root {
        Root {
            graph: Graph::new(),
            bounds,
            module_types: load_metamodules(ctx.clone()),
            context_menu: None,
            ctx,
        }
    }

    fn new_module(&self, meta: &mf::MetaModule<OwnedModule>, rect: Rect2) {
        // dummy z, overwritten by move_to_front
        let bounds = Box3::new(rect.pos.with_z(0.0), rect.size.with_z(0.0));
        let node = self.graph.add_node(meta, bounds);
        self.move_to_front(node.id());
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

    fn compare_node_z(a: &Arc<Node>, b: &Arc<Node>) -> Ordering {
        let a_z = a.module().get().bounds().pos.z;
        let b_z = b.module().get().bounds().pos.z;
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
            let mut module = node.module().get();
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
        let graph_nodes = self.graph.node_map();
        for (id, node) in &graph_nodes {
            let mut module = node.module().get();
            module.render(device, ctx);
        }

        // render global widgets
        if let Some(menu) = self.context_menu.as_mut() {
            menu.render(device, ctx);
        }
    }
    fn handle(&mut self, event: &Event) {
        match event.data {
            EventData::MouseMove(pos) | EventData::Click(pos, _, _) => {
                // march from front to back, if we hit something set this flag so that we only send
                // one local event
                let mut hit = false;

                // intersect menu
                if let Some(menu) = self.context_menu.as_mut() {
                    if event.scope == Scope::Global {
                        menu.handle(event); // assume global events are boring
                    } else if menu.intersect(pos) {
                        hit = true;
                        let status = menu.handle(event);
                        match status {
                            MenuUpdate::Select(path) => {
                                let name: &str = path[0].as_ref();
                                if let Some(module) = self.module_types.iter().find(|ty| ty.name() == name) {
                                    self.new_module(module, Rect2::new(pos, 256.0.into()));
                                } else {
                                    println!("Couldn't find module {}", name);
                                }
                                self.context_menu = None;
                            }
                            _ => (),
                        }
                    }
                }

                // intersect nodes
                let mut nodes = self.graph.nodes();
                nodes.sort_by(Self::compare_node_z);
                for node in &nodes {
                    let mut module = node.module().get();
                    if event.scope == Scope::Global {
                        module.handle(event); // assume global events are boring
                    } else if !hit && module.intersect(pos) {
                        hit = true;
                        let status = module.handle(&event);
                        drop(module); // move_to_front will lock it again
                        if let EventData::Click(_, _, _) = event.data {
                            match status {
                                GuiModuleUpdate::Closed => {
                                    self.graph.remove_node(node.id()).unwrap();
                                }
                                _ => {}
                            }
                            self.move_to_front(node.id());
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

fn load_metamodules(ctx: RenderContext) -> Vec<mf::MetaModule<OwnedModule>> {
    let mut modules = Vec::new();
    let mod_ctx = ctx;
    let test_module = mf::MetaModule::new(
        "TestModule",
        Arc::new(move |ifc, bounds| OwnedModule {
            value: RefCell::new(Box::new(GuiModuleWrapper::new(
                TestModule::new(ifc),
                mod_ctx.clone(),
                bounds,
            )) as Box<GuiComponent<GuiModuleUpdate>>),
        }),
    );
    modules.push(test_module);
    modules
}
