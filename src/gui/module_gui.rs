use super::geom::*;
use super::render::*;
use super::super::module::*;
use super::button::*;
use super::component::*;
use super::event::*;

use gfx_device_gl as gl;

const TITLE_BAR_HEIGHT: f32 = 24.0;
const BORDER_SIZE: f32 = 1.0;

pub struct GuiModuleWrapper<T: Module> {
    module: T,

    target: TextureTarget,

    delete_button: Button,
    bounds: Box3,
    drag: Option<Pt2>,
    dirty: bool,
}

impl<T: Module> GuiModuleWrapper<T> {
    pub fn new(module: T, ctx: RenderContext, bounds: Box3) -> GuiModuleWrapper<T> {
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
                if let Some(drag) = self.drag {
                    self.bounds.pos.x = -drag.x + pos.x;
                    self.bounds.pos.y = -drag.y + pos.y;
                }
                self.handle_delete_button(event.translate(-origin))
            }
            EventData::Click(pos, button, state) => {
                if self.delete_button.intersect(pos - origin) {
                    self.handle_delete_button(event.translate(-origin))
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
