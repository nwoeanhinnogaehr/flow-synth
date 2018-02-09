use super::geom::*;

use glutin;

#[derive(Copy, Clone, Debug)]
pub struct Event {
    pub time: f32,
    pub focus: bool,
    pub data: EventData,
}

impl Event {
    pub fn translate(mut self, offset: Pt2) -> Event {
        match &mut self.data {
            EventData::MouseMove(ref mut pos) | EventData::Click(ref mut pos, _, _) => *pos = *pos + offset,
        }
        self
    }
    pub fn with_focus(mut self, focus: bool) -> Event {
        self.focus = focus;
        self
    }
}

#[derive(Copy, Clone, Debug)]
pub enum EventData {
    MouseMove(Pt2),
    Click(Pt2, MouseButton, ButtonState),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ButtonState {
    Pressed,
    Released,
}

impl<'a> From<&'a glutin::ElementState> for ButtonState {
    fn from(val: &glutin::ElementState) -> ButtonState {
        match val {
            glutin::ElementState::Pressed => ButtonState::Pressed,
            glutin::ElementState::Released => ButtonState::Released,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

impl<'a> From<&'a glutin::MouseButton> for MouseButton {
    fn from(val: &glutin::MouseButton) -> MouseButton {
        match val {
            glutin::MouseButton::Left => MouseButton::Left,
            glutin::MouseButton::Right => MouseButton::Right,
            glutin::MouseButton::Middle => MouseButton::Middle,
            glutin::MouseButton::Other(id) => MouseButton::Other(*id),
        }
    }
}
