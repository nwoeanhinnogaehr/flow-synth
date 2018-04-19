// components are
// - composable
// - embeddable
// - lazy
// - cheap
// - homogenous
//

use gui::{event::Event, geom::*, render::RenderContext};

use gfx_device_gl as gl;

pub trait GuiComponent<Status = ()> {
    fn set_bounds(&mut self, bounds: Box3);
    fn bounds(&self) -> Box3;
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds().flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext);
    fn handle(&mut self, event: &Event) -> Status;
}
