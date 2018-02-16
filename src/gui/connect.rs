use super::geom::*;
use super::component::*;
use super::event::*;
use super::render::*;

use gfx_device_gl as gl;

use std::rc::{Rc, Weak};
use std::cell::{Cell, RefCell};

/// Jacks are GUI connection points
/// wires (pipes) can connect jacks
pub struct Jack {
    jack_ctx: Weak<JackContext>,
    label: String,
    bounds: Cell<Box3>,
    origin: Cell<Pt3>,
    connection: RefCell<Connection>,
}

enum Connection {
    None,
    /// head/tail distinction is so that only one endpoint draws the wire
    Head {
        endpoint: Weak<Jack>,
    },
    Tail {
        endpoint: Weak<Jack>,
    },
    /// floating is a connection in process
    Floating {
        pos: Pt2,
    },
}
impl Connection {
    fn is_connected(&self) -> bool {
        self.endpoint().is_some()
    }
    fn endpoint(&self) -> Option<Rc<Jack>> {
        match self {
            Connection::Head {
                endpoint,
            }
            | Connection::Tail {
                endpoint,
            } => endpoint.upgrade(),
            _ => None,
        }
    }
    fn disconnect(&mut self) {
        if self.is_connected() {
            self.endpoint()
                .map(|endpoint| endpoint.connection.replace(Connection::None));
            *self = Connection::None;
        }
    }
}

impl Jack {
    fn new(jack_ctx: &Rc<JackContext>, label: String, bounds: Box3, origin: Pt3) -> Jack {
        Jack {
            jack_ctx: Rc::downgrade(jack_ctx),
            label,
            bounds: Cell::new(bounds),
            origin: Cell::new(origin),
            connection: RefCell::new(Connection::None),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn origin(&self) -> Pt3 {
        self.origin.get()
    }
    pub fn set_origin(&self, origin: Pt3) {
        self.origin.set(origin);
    }
    pub fn connection_point(&self) -> Pt3 {
        self.bounds.get().pos + self.bounds.get().size.y / 2.0 + self.origin()
    }
}
impl GuiComponent for Rc<Jack> {
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds.set(bounds);
    }
    fn bounds(&self) -> Box3 {
        self.bounds.get()
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds().flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        const BORDER_SIZE: f32 = 1.0;
        const PAD: f32 = 2.0;
        let port_size = self.bounds().size.y;
        ctx.draw_rect(
            Rect3::new(
                self.bounds().pos + Pt3::new(PAD, PAD, 0.0),
                (port_size - PAD * 2.0).into(),
            ),
            [1.0, 1.0, 1.0],
        );
        ctx.draw_rect(
            Rect3::new(
                self.bounds().pos + Pt3::new(PAD + BORDER_SIZE, PAD + BORDER_SIZE, 0.0),
                (port_size - PAD * 2.0 - BORDER_SIZE * 2.0).into(),
            ),
            [0.0, 0.0, 0.5],
        );
        ctx.draw_text(
            self.label(),
            self.bounds().pos + Pt3::new(20.0, 0.0, 0.0),
            [1.0, 1.0, 1.0],
        );
    }
    fn handle(&mut self, event: &Event) {
        match event.data {
            EventData::Click(pos, button, state) => {
                if event.focus && state == ButtonState::Pressed && button == MouseButton::Left
                    && self.bounds().flatten().drop_z().intersect(pos)
                {
                    let ctx = self.jack_ctx
                        .upgrade()
                        .expect("no events expected during shutdown");
                    let mut in_progress = ctx.in_progress.borrow_mut();
                    let mut connection = self.connection.borrow_mut();
                    if let Some(ref mut weak_in_progress) = in_progress.take() {
                        if let Some(ref mut endpoint) = weak_in_progress.upgrade() {
                            if !Rc::ptr_eq(endpoint, self) {
                                // connection in progress:
                                // click establishes new connection
                                connection.disconnect();
                                *connection = Connection::Tail {
                                    endpoint: weak_in_progress.clone(),
                                };
                                *endpoint.connection.borrow_mut() = Connection::Head {
                                    endpoint: Rc::downgrade(self),
                                };
                            } else {
                                // replace in_progress (taken above) if we decide to bail out
                                *in_progress = Some(weak_in_progress.clone());
                            }
                        }
                    } else {
                        // begin connecting
                        connection.disconnect();
                        *connection = Connection::Floating {
                            pos: pos + self.origin().drop_z(),
                        };
                        *in_progress = Some(Rc::downgrade(self));
                    }
                }
            }
            EventData::MouseMove(mouse_pos) => match *self.connection.borrow_mut() {
                Connection::Floating {
                    ref mut pos,
                } => {
                    *pos = mouse_pos + self.origin().drop_z();
                }
                _ => {}
            },
        }
    }
}

/// a Jack is unaware of other nearby jacks it can connect to
/// so the JackContext manages establishment of connections between jacks
/// (within a specific context)
pub struct JackContext {
    bounds: Cell<Box3>,
    jacks: RefCell<Vec<Weak<Jack>>>,
    in_progress: RefCell<Option<Weak<Jack>>>,
}

impl JackContext {
    pub fn new(bounds: Box3) -> Rc<JackContext> {
        Rc::new(JackContext {
            bounds: Cell::new(bounds),
            jacks: RefCell::new(Vec::new()),
            in_progress: RefCell::new(None),
        })
    }
    pub fn new_jack(self: &Rc<JackContext>, label: String, bounds: Box3, origin: Pt3) -> Rc<Jack> {
        let jack = Rc::new(Jack::new(&self, label, bounds, origin));
        let mut jacks = self.jacks.borrow_mut();
        weak_cleanup(&mut jacks);
        jacks.push(Rc::downgrade(&jack));
        jack
    }
}

/// Drop dropped Weak Ts
fn weak_cleanup<T>(vec: &mut Vec<Weak<T>>) {
    vec.retain(|x| x.upgrade().is_some());
}

/// the wires fill space outside of the modules, so they are rendered by the context
impl GuiComponent for Rc<JackContext> {
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds.set(bounds);
    }
    fn bounds(&self) -> Box3 {
        self.bounds.get()
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.bounds().flatten().drop_z().intersect(pos)
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        let jacks = self.jacks.borrow();
        for jack in jacks.iter() {
            if let Some(jack) = jack.upgrade() {
                match *jack.connection.borrow() {
                    Connection::Head {
                        ref endpoint,
                    } => {
                        if let Some(endpoint) = endpoint.upgrade() {
                            ctx.draw_pipe(&[
                                jack.connection_point().drop_z().with_z(0.0),
                                endpoint.connection_point().drop_z().with_z(0.0),
                            ]);
                        }
                    }
                    Connection::Floating {
                        pos,
                    } => {
                        ctx.draw_pipe(&[
                            jack.connection_point().drop_z().with_z(0.0),
                            pos.with_z(0.0),
                        ]);
                    }
                    _ => {}
                }
            }
        }
    }
    fn handle(&mut self, event: &Event) {}
}
