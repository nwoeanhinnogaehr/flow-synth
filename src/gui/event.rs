use gui::geom::*;

use glutin;

pub use glutin::VirtualKeyCode;

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
            _ => {}
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
    Key(KeyEvent),
    Character(char),
}

#[derive(Copy, Clone, Debug)]
pub struct KeyEvent {
    pub code: VirtualKeyCode,
    pub modifiers: KeyModifiers,
    pub state: ButtonState,
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl<'a> From<&'a glutin::ModifiersState> for KeyModifiers {
    fn from(val: &glutin::ModifiersState) -> KeyModifiers {
        KeyModifiers {
            shift: val.shift,
            ctrl: val.ctrl,
            alt: val.alt,
            logo: val.logo,
        }
    }
}

