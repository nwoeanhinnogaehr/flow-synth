// components are
// - composable
// - embeddable
// - lazy
// - cheap
// - homogenous
//

use super::geom::*;
use super::render::RenderContext;
use super::event::Event;

use gfx_device_gl as gl;

pub trait GuiComponent<Status = ()> {
    fn set_bounds(&mut self, bounds: Box3);
    fn bounds(&self) -> Box3;
    fn intersect(&self, pos: Pt2) -> bool;
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext);
    fn handle(&mut self, event: &Event) -> Status;
}
