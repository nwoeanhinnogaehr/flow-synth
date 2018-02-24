use super::geom::*;
use super::render::*;
use super::super::module::*;
use super::button::*;
use super::component::*;
use super::event::*;
use super::connect::*;

use modular_flow as mf;

use std::rc::Rc;
use std::sync::Arc;

use gfx_device_gl as gl;

pub struct GuiModuleConfig {
    pub graph: Arc<mf::Graph>,
    pub ctx: RenderContext,
    pub bounds: Box3,
    pub jack_ctx: Rc<JackContext<Arc<mf::Port>>>,
}
pub trait GuiModuleFactory {
    fn name(&self) -> &str;
    fn new(&mut self, arg: GuiModuleConfig) -> Box<GuiModule>;
}

pub struct BasicGuiModuleFactory<F: FnMut(GuiModuleConfig) -> Box<GuiModule>> {
    name: &'static str,
    new_fn: F,
}

impl<F: FnMut(GuiModuleConfig) -> Box<GuiModule>> BasicGuiModuleFactory<F> {
    pub fn new(name: &'static str, new_fn: F) -> BasicGuiModuleFactory<F> {
        BasicGuiModuleFactory {
            name,
            new_fn,
        }
    }
}

impl<F> GuiModuleFactory for BasicGuiModuleFactory<F>
where
    F: FnMut(GuiModuleConfig) -> Box<GuiModule>,
{
    fn name(&self) -> &str {
        self.name
    }
    fn new(&mut self, arg: GuiModuleConfig) -> Box<GuiModule> {
        (self.new_fn)(arg)
    }
}

pub trait GuiModule: GuiComponent<GuiModuleUpdate> {
    fn node(&self) -> Arc<mf::Node>;
}

const TITLE_BAR_HEIGHT: f32 = 24.0;
const BORDER_SIZE: f32 = 1.0;

pub struct GuiModuleWrapper<T: Module> {
    module: T,
    node: Arc<mf::Node>,

    target: TextureTarget,

    delete_button: Button,
    jacks: Vec<Rc<Jack<Arc<mf::Port>>>>,
    bounds: Box3,
    drag: Option<Pt2>,
    dirty: bool,
}

impl<T: Module> GuiModuleWrapper<T> {
    pub fn new(cfg: GuiModuleConfig) -> GuiModuleWrapper<T> {
        let GuiModuleConfig {
            bounds,
            jack_ctx,
            ctx,
            graph,
        } = cfg;
        let target = TextureTarget::new(ctx.clone(), bounds.size.drop_z());
        let ifc = graph.add_node();
        let node = graph.node(ifc.id()).unwrap();
        let mut module = T::new(ifc);

        let jacks = module
            .ports()
            .iter()
            .enumerate()
            .map(|(idx, port)| {
                let pos = Pt3::new(4.0, 4.0 + TITLE_BAR_HEIGHT + idx as f32 * 20.0, 0.8);
                let size = Pt3::new(bounds.size.x, 20.0, 0.1);
                let jack_bounds = Box3::new(pos, size);
                jack_ctx.new_jack(port.clone(), jack_bounds, bounds.pos)
            })
            .collect();

        module.start();

        GuiModuleWrapper {
            module,
            node,
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
        let title = &self.module.title();
        self.target
            .ctx()
            .draw_text(title, Pt3::new(4.0, 4.0, 0.8), [1.0, 1.0, 1.0]);

        for jack in &mut self.jacks {
            jack.render(device, self.target.ctx());
        }
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
    fn node(&self) -> Arc<mf::Node> {
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
        }
    }
}
