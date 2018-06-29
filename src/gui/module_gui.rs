use gui::{button::*, component::*, connect::*, event::*, geom::*, render::*, layout};
use module::*;

use futures::executor::ThreadPool;

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use gfx_device_gl as gl;

pub struct GuiModuleConfig {
    pub graph: Arc<flow::Graph>,
    pub ctx: RenderContext,
    pub bounds: Box3,
    pub jack_ctx: Rc<JackContext<Arc<flow::OpaquePort>>>,
    pub executor: ThreadPool,
}
pub trait GuiModuleFactory {
    fn name(&self) -> &str;
    fn new(&mut self, arg: GuiModuleConfig) -> Box<dyn GuiModule>;
}

#[derive(Default)]
pub struct BasicGuiModuleFactory<T: Module + 'static> {
    _t: PhantomData<T>,
}
impl<T: Module + 'static> BasicGuiModuleFactory<T> {
    pub fn new() -> BasicGuiModuleFactory<T> {
        BasicGuiModuleFactory {
            _t: PhantomData,
        }
    }
}
impl<T: Module + 'static> GuiModuleFactory for BasicGuiModuleFactory<T> {
    fn name(&self) -> &str {
        T::name()
    }
    fn new(&mut self, cfg: GuiModuleConfig) -> Box<dyn GuiModule> {
        Box::new(GuiModuleWrapper::<T>::new(cfg))
    }
}

pub trait GuiModule: GuiComponent<GuiModuleUpdate> {
    fn node(&self) -> Arc<flow::Node>;
}

pub type BodyUpdate = bool;
pub trait ModuleGui {
    fn new_body(&mut self, ctx: &mut RenderContext, bounds: Box3) -> Box<dyn GuiComponent<BodyUpdate>>;
}
impl<T> ModuleGui for T {
    default fn new_body(
        &mut self,
        ctx: &mut RenderContext,
        bounds: Box3,
    ) -> Box<dyn GuiComponent<BodyUpdate>> {
        Box::new(NullComponent {})
    }
}

const TITLE_BAR_HEIGHT: f32 = 24.0;
const BORDER_SIZE: f32 = 1.0;
const JACK_HEIGHT: f32 = 20.0;

struct NullComponent {}
impl<T: Default> GuiComponent<T> for NullComponent {
    fn set_bounds(&mut self, bounds: Box3) {}
    fn bounds(&self) -> Box3 {
        Box3::default()
    }
    fn intersect(&self, pos: Pt2) -> bool {
        false
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {}
    fn handle(&mut self, event: &Event) -> T {
        T::default()
    }
}

pub struct GuiModuleWrapper<T: Module + 'static> {
    module: T,
    node: Arc<flow::Node>,

    target: TextureTarget,
    body: Box<dyn GuiComponent<BodyUpdate>>,

    delete_button: Button,
    jacks: Vec<Rc<Jack<Arc<flow::OpaquePort>>>>,
    bounds: Box3,
    drag: Option<Pt2>,
    dirty: bool,
}

impl<T: Module + 'static> Drop for GuiModuleWrapper<T> {
    fn drop(&mut self) {
        self.module.stop();
    }
}

impl<T: Module + 'static> GuiModuleWrapper<T> {
    pub fn new(cfg: GuiModuleConfig) -> GuiModuleWrapper<T> {
        let GuiModuleConfig {
            bounds,
            jack_ctx,
            mut ctx,
            graph,
            executor,
        } = cfg;
        let target = TextureTarget::new(ctx.clone(), bounds.size.drop_z());
        let ifc = graph.add_node();
        let node = graph.node(ifc.id()).unwrap();
        let mut module = T::new(ifc);
        let ports = module.ports();

        // Bounds pos is relative to the window, so we drop it and keep just the size,
        // for the purposes of the layout solver
        let mut solver = layout::Layout::new(Box3::new(0.0.into(), bounds.size));

        // set up two main areas, for the jacks and the body
        let jack_area = solver.add_node();
        let body_area = solver.add_node();
        solver.stack(layout::Axis::Y, &[jack_area, body_area]);
        solver.suggest(jack_area, layout::Field::Height, ports.len() as f64 * JACK_HEIGHT as f64, layout::REQUIRED);
        solver.suggest(jack_area, layout::Field::Y, TITLE_BAR_HEIGHT as f64, layout::REQUIRED);

        // stack all jacks vertically inside the jack area
        let jack_layouts = solver.add_nodes(ports.len());
        solver.equalize(layout::Field::Height, &jack_layouts, layout::REQUIRED);
        solver.stack(layout::Axis::Y, &jack_layouts);
        solver.insert_inside(jack_area, &jack_layouts);
        let jacks: Vec<_> = ports
            .iter()
            .zip(jack_layouts)
            .map(|(port, layout)| {
                jack_ctx.new_jack(port.clone(), solver.query(layout), bounds.pos)
            })
            .collect();

        let body = module.new_body(&mut ctx, solver.query(body_area));

        module.start(executor);

        GuiModuleWrapper {
            module,
            node,
            target,
            body,
            delete_button: Button::new(
                ctx,
                "X".into(),
                Box3 {
                    pos: Pt3::new(bounds.size.x - TITLE_BAR_HEIGHT - BORDER_SIZE, BORDER_SIZE, 0.0),
                    size: Pt3::new(TITLE_BAR_HEIGHT, TITLE_BAR_HEIGHT, 0.0),
                },
            ),
            jacks,
            bounds,
            drag: None,
            dirty: true,
        }
    }
    fn render_self(&mut self, device: &mut gl::Device) {
        // borders
        self.target.ctx().draw_rect(
            Rect3::new(Pt3::new(0.0, 0.0, 1.0), self.bounds.size.drop_z()),
            [1.0, 1.0, 1.0],
        );
        // background
        self.target.ctx().draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE + TITLE_BAR_HEIGHT, 0.9),
                self.bounds.size.drop_z() - Pt2::new(BORDER_SIZE * 2.0, BORDER_SIZE * 2.0 + TITLE_BAR_HEIGHT),
            ),
            [0.1, 0.1, 0.1],
        );
        // title bar
        self.target.ctx().draw_rect(
            Rect3::new(
                Pt3::new(BORDER_SIZE, BORDER_SIZE, 0.9),
                Pt2::new(self.bounds.size.x - BORDER_SIZE * 2.0, TITLE_BAR_HEIGHT),
            ),
            [0.0, 0.0, 0.0],
        );
        let title = T::name();
        self.target
            .ctx()
            .draw_text(title, Pt3::new(4.0, 4.0, 0.8), [1.0, 1.0, 1.0]);

        for jack in &mut self.jacks {
            jack.render(device, self.target.ctx());
        }
        self.body.render(device, self.target.ctx());
    }
    fn handle_delete_button(&mut self, event: Event) -> GuiModuleUpdate {
        match self.delete_button.handle(&event) {
            ButtonUpdate::NeedRender => {
                self.dirty = true;
                GuiModuleUpdate::Unchanged
            }
            ButtonUpdate::Clicked => GuiModuleUpdate::Closed,
            ButtonUpdate::Unchanged => GuiModuleUpdate::Unchanged,
        }
    }
}

impl<T: Module> GuiModule for GuiModuleWrapper<T> {
    fn node(&self) -> Arc<flow::Node> {
        self.node.clone()
    }
}

pub enum GuiModuleUpdate {
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
        for jack in &mut self.jacks {
            jack.set_origin(bounds.pos);
        }
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds.flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        if self.dirty {
            self.target.begin_frame();
            self.render_self(device);
            self.delete_button.render(device, self.target.ctx());
            self.target.end_frame(device);
            self.dirty = false;
        }

        ctx.draw_textured_rect(self.bounds.flatten(), self.target.shader_resource().clone());
    }
    fn handle(&mut self, event: &Event) -> GuiModuleUpdate {
        let origin = self.bounds.pos.drop_z();
        for jack in &mut self.jacks {
            jack.handle(&event.translate(-origin));
        }
        self.dirty |= self.body.handle(&event.translate(-origin));
        match event.data {
            EventData::MouseMove(pos) => {
                if let Some(drag) = self.drag {
                    let mut bounds = self.bounds();
                    bounds.pos.x = -drag.x + pos.x;
                    bounds.pos.y = -drag.y + pos.y;
                    self.set_bounds(bounds);
                }
                self.handle_delete_button(event.translate(-origin))
            }
            EventData::Click(pos, button, state) => {
                if event.focus && !self.delete_button.intersect(pos - origin) {
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
                }
                self.handle_delete_button(event.translate(-origin))
            }
            _ => GuiModuleUpdate::Unchanged,
        }
    }
}
