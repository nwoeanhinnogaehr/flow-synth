use gui::{component::*, event::*, geom::*, RenderContext};

use gfx_device_gl as gl;

const BORDER_SIZE: f32 = 1.0;

pub struct TextBox {
    content: String,
    bounds: Box3,

    cursor: usize,
    focused: bool,
}

impl TextBox {
    pub fn new(ctx: RenderContext, content: String, bounds: Box3) -> TextBox {
        TextBox {
            content,
            bounds,
            cursor: 0,
            focused: false,
        }
    }
    pub fn set_content(&mut self, content: String) {
        self.content = content;
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextBoxUpdate {
    Unchanged,
    NeedRender,
    Modified,
}

impl GuiComponent<TextBoxUpdate> for TextBox {
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
            if self.focused { [0.1, 0.1, 0.3] } else { [0.1; 3] },
        );
        // cursor
        ctx.draw_rect(
            Rect3::new(
                self.bounds.pos + Pt3::new(4.0 + self.cursor as f32 * 10.0, 0.0, 0.0),
                Pt2::new(10.0, self.bounds.size.y),
            ),
            if self.focused { [0.1, 0.3, 0.1] } else { [0.2; 3] },
        );
        ctx.draw_text(&self.content, self.bounds.pos + Pt3::new(4.0, 4.0, 0.0), [1.0; 3]);
    }
    fn handle(&mut self, event: &Event) -> TextBoxUpdate {
        match event.data {
            EventData::Click(pos, button, state)
                if button == MouseButton::Left && state == ButtonState::Pressed =>
            {
                self.focused = event.focus;
                TextBoxUpdate::NeedRender
            }
            EventData::Key(kev) if self.focused => {
                if kev.state == ButtonState::Pressed {
                    match kev.code {
                        VirtualKeyCode::Left if self.cursor > 0 => self.cursor -= 1,
                        VirtualKeyCode::Right if self.cursor < self.content.len() => self.cursor += 1,
                        _ => {}
                    }
                }
                TextBoxUpdate::NeedRender
            }
            EventData::Character(ch) if self.focused => {
                if ch == '\x08' {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.content.remove(self.cursor);
                    }
                } else {
                    self.content.insert(self.cursor, ch);
                    self.cursor += 1;
                }
                TextBoxUpdate::NeedRender
            }
            _ => TextBoxUpdate::Unchanged,
        }
    }
}
