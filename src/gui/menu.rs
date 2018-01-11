use super::{GuiElement, Model, Rect, RenderContext};

use gfx_device_gl as gl;
use glutin;

#[derive(Clone)]
pub enum MenuItem {
    Item(String),
    SubMenu(String, Menu),
}

#[derive(Clone)]
pub struct Menu {
    items: Vec<MenuItem>,
}

impl Menu {
    pub fn of(items: &[MenuItem]) -> Menu {
        Menu {
            items: items.into(),
        }
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32) {}
    fn intersect(&self, point: [f32; 2]) -> bool {
        false
    }
}

pub fn item(text: &str) -> MenuItem {
    MenuItem::Item(text.into())
}
pub fn sub_menu(text: &str, items: &[MenuItem]) -> MenuItem {
    MenuItem::SubMenu(text.into(), Menu::of(items))
}

#[test]
fn test_construct_menu() {
    use self::MenuItem::*;
    let menu = Menu::of(&[
        item("foo"),
        item("bar"),
        sub_menu("baz", &[item("abc"), item("def"), item("ghi")]),
        item("2000"),
    ]);
}

// only one menu open at a time,
// but there can be submenus
pub struct MenuManager {
    menu: Option<Menu>,
}

impl MenuManager {
    pub fn new() -> MenuManager {
        MenuManager { menu: None }
    }
}

impl GuiElement for MenuManager {
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32) {
        self.menu
            .as_mut()
            .map(|menu| menu.render(device, ctx, depth));
    }
    fn intersect(&self, point: [f32; 2]) -> bool {
        self.menu
            .as_ref()
            .map(|menu| menu.intersect(point))
            .unwrap_or(false)
    }
    fn update(&mut self, model: &Model) {}
    fn handle(&mut self, model: &Model, event: &glutin::Event) {}
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton) {}
}
