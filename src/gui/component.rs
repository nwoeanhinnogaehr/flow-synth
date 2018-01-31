// components are
// - composable
// - embeddable
// - lazy
// - cheap
// - homogenous
//
use super::geom::*;
use super::render::RenderContext;
use super::Model;

use glutin;
use gfx_device_gl as gl;

pub trait GuiComponent<Status> {
    fn rect(&self) -> Rect2;
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, parent: Rect3);
    fn update(&mut self, model: &Model) -> Status;
    fn handle(&mut self, model: &Model, event: &glutin::Event) -> Status;
}
