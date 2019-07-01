//! Root component that holds the application

use gui::{component::*, connect::*, event::*, geom::*, menu::*, module_gui::*, render::*};
use module::flow;

use futures::executor::ThreadPool;
use gfx_device_gl as gl;
use ron;

use std::cmp::Ordering;
use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;

pub struct Root {
    graph: Arc<flow::Graph>,
    bounds: Box3,

    ctx: RenderContext,
    modules: Vec<Box<dyn GuiModule>>,
    module_types: Vec<Box<dyn GuiModuleFactory>>,
    context_menu: Option<MenuView>,
    jack_ctx: Rc<JackContext<Arc<flow::OpaquePort>>>,
    executor: ThreadPool,
}

impl Root {
    pub fn new(ctx: RenderContext, bounds: Box3) -> Root {
        Root {
            graph: flow::Graph::new(),
            bounds,
            modules: Vec::new(),
            module_types: load_metamodules(),
            context_menu: None,
            jack_ctx: JackContext::new(bounds),
            executor: ThreadPool::new().unwrap(),

            ctx,
        }
    }

    fn new_module(
        &mut self,
        name: &str,
        bounds: Box3,
        node_id: Option<flow::NodeId>,
    ) -> Result<flow::NodeId, ()> {
        // dummy z, overwritten by move_to_front
        if let Some(factory) = self.module_types.iter_mut().find(|ty| ty.name() == name) {
            let module = factory.new(GuiModuleConfig {
                bounds,
                jack_ctx: Rc::clone(&self.jack_ctx),
                graph: Arc::clone(&self.graph),
                ctx: self.ctx.clone(),
                executor: self.executor.clone(),
                node_id,
            });
            let id = module.node().id();
            self.modules.push(module);
            Ok(id)
        } else {
            Err(())
        }
    }

    fn open_new_module_menu(&mut self, pos: Pt2) {
        self.context_menu = Some(MenuView::new(
            self.ctx.clone(),
            Box3::new(pos.with_z(0.0), (self.bounds.size.drop_z() - pos).with_z(0.0)),
            Menu::new(
                &self
                    .module_types
                    .iter()
                    .map(|ty| item(&ty.name()))
                    .collect::<Vec<_>>(),
            ),
        ));
    }

    fn compare_node_z(a: &Box<dyn GuiModule>, b: &Box<dyn GuiModule>) -> Ordering {
        let a_z = a.bounds().pos.z;
        let b_z = b.bounds().pos.z;
        a_z.partial_cmp(&b_z).unwrap()
    }

    fn move_to_front(&mut self, id: flow::NodeId) {
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

    fn save(&self, filename: &str) -> Result<(), serial::Error> {
        use std::collections::HashSet;
        use std::io::prelude::*;

        let mut modules = Vec::new();
        let mut connections = Vec::new();
        // keep track of visited ports so we only serialize one end of the connection
        let mut visited_ports = HashSet::new();
        for module in &self.modules {
            let bounds = module.bounds();
            let node = module.node();
            let module = serial::Module {
                bounds,
                id: node.id(),
                type_name: module.name().into(),
            };
            modules.push(module);

            for port in node.ports() {
                visited_ports.insert((port.node_id(), port.id()));
                if let Some(dst) = port.edge() {
                    if !visited_ports.contains(&(dst.node_id(), dst.id())) {
                        visited_ports.insert((dst.node_id(), dst.id()));
                        let connection = serial::Connection {
                            src_node: node.id(),
                            src_port: port.name().into(),
                            dst_node: dst.node_id(),
                            dst_port: dst.name().into(),
                        };
                        connections.push(connection);
                    }
                }
            }
        }
        let root = serial::Root {
            modules,
            connections,
        };

        let data = ron::ser::to_string(&root).unwrap();
        let mut file = File::create(filename)?;
        write!(file, "{}", data)?;

        Ok(())
    }

    fn load(&mut self, filename: &str) -> ron::de::Result<()> {
        // reset current state
        ::std::mem::replace(self, Root::new(self.ctx.clone(), self.bounds));

        let file = File::open(filename)?;
        let root: serial::Root = ron::de::from_reader(file)?;

        for module in root.modules {
            if let Err(_) = self.new_module(&module.type_name, module.bounds, Some(module.id)) {
                println!("Error creating module {:?}", module.type_name);
            }
        }

        for connection in root.connections {
            let src_node = self
                .modules
                .iter()
                .find(|module| module.node().id() == connection.src_node)
                .unwrap();
            let dst_node = self
                .modules
                .iter()
                .find(|module| module.node().id() == connection.dst_node)
                .unwrap();
            let src_jack = src_node
                .jacks()
                .iter()
                .find(|jack| jack.name() == connection.src_port);
            let dst_jack = dst_node
                .jacks()
                .iter()
                .find(|jack| jack.name() == connection.dst_port);
            if let (Some(src_jack), Some(dst_jack)) = (src_jack, dst_jack) {
                src_jack.connect(dst_jack);
            } else {
                println!(
                    "Could not find port(s) needed to connect {:?}:{:?} and {:?}:{:?}",
                    src_node.name(),
                    connection.src_port,
                    dst_node.name(),
                    connection.dst_port
                );
            }
        }

        Ok(())
    }
}

mod serial {
    use gui::geom::*;
    use module::flow::NodeId;
    use ron;
    use std::io;

    #[derive(Debug)]
    pub enum Error {
        IO(io::Error),
        Serialize(ron::ser::Error),
    }
    impl From<io::Error> for Error {
        fn from(e: io::Error) -> Error {
            Error::IO(e)
        }
    }
    impl From<ron::ser::Error> for Error {
        fn from(e: ron::ser::Error) -> Error {
            Error::Serialize(e)
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Root {
        pub modules: Vec<Module>,
        pub connections: Vec<Connection>,
    }
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Module {
        pub bounds: Box3,
        pub id: NodeId,
        pub type_name: String,
    }
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Connection {
        pub src_node: NodeId,
        pub src_port: String,
        pub dst_node: NodeId,
        pub dst_port: String,
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
            EventData::Key(KeyEvent {
                code: VirtualKeyCode::S,
                modifiers:
                    KeyModifiers {
                        ctrl: true,
                        shift: false,
                        alt: false,
                        logo: false,
                    },
                state: ButtonState::Pressed,
            }) => {
                println!("Save: {:?}", self.save("project.fsy"));
            }
            EventData::Key(KeyEvent {
                code: VirtualKeyCode::L,
                modifiers:
                    KeyModifiers {
                        ctrl: true,
                        shift: false,
                        alt: false,
                        logo: false,
                    },
                state: ButtonState::Pressed,
            }) => {
                println!("Load: {:?}", self.load("project.fsy"));
            }
            EventData::Key(_) | EventData::Character(_) => {
                for module in &mut self.modules {
                    module.handle(&event.with_focus(true));
                }
            }
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
                                let bounds = Box3::new(pos.with_z(0.0), Pt2::from(256.0).with_z(0.0));
                                let id = self.new_module(name, bounds, None).unwrap();
                                self.move_to_front(id);
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
                        if !menu.intersect(pos)
                            && ButtonState::Pressed == state
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

fn load_metamodules() -> Vec<Box<dyn GuiModuleFactory>> {
    use module::audio_io::*;
    use module::debug::*;
    use module::livecode::*;
    vec![
        Box::new(BasicGuiModuleFactory::<Printer<i32>>::new()),
        Box::new(BasicGuiModuleFactory::<Counter<i32>>::new()),
        Box::new(BasicGuiModuleFactory::<AudioIO>::new()),
        Box::new(BasicGuiModuleFactory::<LiveCode>::new()),
    ]
}
