use gui::{component::*, event::*, geom::*, RenderContext};

use gfx_device_gl as gl;

const BORDER_SIZE: f32 = 1.0;

pub struct Button {
    label: String,
    bounds: Box3,

    hover: bool,
    clicking: bool,
}

impl Button {
    pub fn new(ctx: RenderContext, label: String, bounds: Box3) -> Button {
        Button {
            label,
            bounds,
            hover: false,
            clicking: false,
        }
    }
    pub fn set_label(&mut self, label: String) {
        self.label = label;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonUpdate {
    Unchanged,
    NeedRender,
    Clicked,
}

impl GuiComponent<ButtonUpdate> for Button {
    fn bounds(&self) -> Box3 {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds = bounds;
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        // border
        ctx.draw_rect(self.bounds.flatten(), [1.0; 3]);
        // background
        ctx.draw_rect(
            Rect3::new(
                self.bounds.pos + Pt3::new(BORDER_SIZE, BORDER_SIZE, 0.0),
                self.bounds.flatten().size - BORDER_SIZE * 2.0,
            ),
            if self.clicking {
                if self.hover {
                    [0.0, 0.0, 0.3]
                } else {
                    [0.1, 0.1, 0.3]
                }
            } else {
                if self.hover {
                    [0.0; 3]
                } else {
                    [0.1; 3]
                }
            },
        );
        ctx.draw_text(&self.label, self.bounds.pos + Pt3::new(4.0, 4.0, 0.0), [1.0; 3]);
    }
    fn handle(&mut self, event: &Event) -> ButtonUpdate {
        match event.data {
            EventData::MouseMove(pos) => {
                let hover = self.bounds.flatten().drop_z().intersect(pos);
                if self.hover != hover {
                    self.hover = hover;
                    ButtonUpdate::NeedRender
                } else {
                    ButtonUpdate::Unchanged
                }
            }
            EventData::Click(pos, button, state)
                if button == MouseButton::Left && state == ButtonState::Released && self.clicking =>
            {
                self.clicking = false;
                if self.hover {
                    ButtonUpdate::Clicked
                } else {
                    ButtonUpdate::NeedRender
                }
            }
            EventData::Click(pos, button, state)
                if event.focus
                    && button == MouseButton::Left
                    && state == ButtonState::Pressed
                    && self.hover =>
            {
                self.clicking = true;
                ButtonUpdate::NeedRender
            }
            _ => ButtonUpdate::Unchanged,
        }
    }
}
