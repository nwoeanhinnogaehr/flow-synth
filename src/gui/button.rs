use super::{Model, RenderContext};
use super::render::*;
use super::component::*;
use super::geom::*;

use gfx_device_gl as gl;
use glutin;

const BORDER_SIZE: f32 = 1.0;

pub struct Button {
    label: String,
    rect: Rect2,

    target: TextureTarget,
    hover: bool,
}

impl Button {
    pub fn new(ctx: RenderContext, label: String, rect: Rect2) -> Button {
        Button {
            label,
            rect,
            target: TextureTarget::new(ctx, rect.size),
            hover: false,
        }
    }
}

impl GuiComponent<()> for Button {
    fn rect(&self) -> Rect2 {
        self.rect
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, parent: Rect3) {
        // border
        ctx.draw_rect(self.rect.upgrade_with(&parent), [1.0; 3]);
        // background
        ctx.draw_rect(
            Rect2 {
                pos: self.rect.pos + Pt2::fill(BORDER_SIZE),
                size: self.rect.size - Pt2::fill(BORDER_SIZE * 2.0),
            }.upgrade_with(&parent),
            if self.hover { [0.0; 3] } else { [0.1; 3] },
        );
        ctx.draw_text(&self.label, self.rect.pos + Pt2::fill(4.0), [1.0; 3]);

        ctx.draw_textured_rect(
            self.rect.upgrade_with(&parent),
            self.target.shader_resource().clone(),
        );
    }
    fn update(&mut self, model: &Model) {
        let hover = self.rect.intersect(model.mouse_pos);
        self.hover = hover;
    }
    fn handle(&mut self, model: &Model, event: &glutin::Event) {}
}
