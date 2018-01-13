use super::{GuiElement, Model, Rect, RenderContext};
use super::render::*;

use gfx_device_gl as gl;
use glutin;

const ITEM_HEIGHT: f32 = 32.0;
const ITEM_WIDTH: f32 = 128.0;
const BORDER_SIZE: f32 = 1.0;

#[derive(Clone)]
pub struct MenuItem {
    label: String,
    sub_menu: Option<Menu>,
    hover: bool,
}

impl MenuItem {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn sub_menu(&self) -> Option<&Menu> {
        self.sub_menu.as_ref()
    }
    pub fn sub_menu_mut(&mut self) -> Option<&mut Menu> {
        self.sub_menu.as_mut()
    }
}
pub fn item(label: &str) -> MenuItem {
    MenuItem {
        label: label.into(),
        sub_menu: None,
        hover: false,
    }
}
pub fn sub_menu(label: &str, items: &[MenuItem]) -> MenuItem {
    MenuItem {
        label: label.into(),
        sub_menu: Some(Menu::new(items)),
        hover: false,
    }
}

#[derive(Clone)]
pub struct Menu {
    items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(items: &[MenuItem]) -> Menu {
        Menu {
            items: items.into(),
        }
    }
    pub fn length(&self) -> usize {
        self.items.len()
    }
    pub fn width(&self) -> usize {
        self.items.iter().fold(1, |acc, item| {
            item.sub_menu()
                .map(|child| acc.max(1 + child.width()))
                .unwrap_or(acc)
        })
    }
}

#[test]
fn test_menu() {
    use self::MenuItem::*;
    let menu = Menu::new(&[
        item("foo"),
        item("bar"),
        sub_menu("baz", &[item("abc"), item("def"), item("ghi")]),
        sub_menu(
            "baz2",
            &[
                item("abc"),
                sub_menu("def", &[item("xyz"), item("zzz")]),
                item("ghi"),
            ],
        ),
        item("2000"),
    ]);
    assert_eq!(menu.length(), 5);
    assert_eq!(menu.width(), 3);
}

pub struct MenuView {
    menu: Menu,
    pos: [f32; 2],
    target: TextureTarget,
    dirty: bool,
}

impl MenuView {
    pub fn new(ctx: RenderContext, pos: [f32; 2], menu: Menu) -> MenuView {
        let size = [
            ITEM_WIDTH * menu.width() as f32,
            ITEM_HEIGHT * menu.length() as f32,
        ];
        MenuView {
            menu,
            pos,
            target: TextureTarget::new(ctx, size),
            dirty: true,
        }
    }
    fn render_menu(ctx: &mut RenderContext, menu: &mut Menu, offset: [f32; 2]) {
        for (idx, item) in menu.items.iter_mut().enumerate() {
            let pos = [
                offset[0],
                offset[1] + idx as f32 * (ITEM_HEIGHT - BORDER_SIZE),
            ];
            // borders
            ctx.draw_rect(ColoredRect {
                translate: [pos[0], pos[1], 0.0],
                scale: [ITEM_WIDTH, ITEM_HEIGHT],
                color: [1.0, 1.0, 1.0],
            });
            // background
            ctx.draw_rect(ColoredRect {
                translate: [pos[0] + BORDER_SIZE, pos[1] + BORDER_SIZE, 0.0],
                scale: [
                    ITEM_WIDTH - BORDER_SIZE * 2.0,
                    ITEM_HEIGHT - BORDER_SIZE * 2.0,
                ],
                color: [0.1, 0.1, 0.1],
            });
            ctx.draw_text(&item.label(), [pos[0] + 4.0, pos[1] + 4.0], [1.0, 1.0, 1.0]);

            if item.hover {
                if let Some(ref mut menu) = item.sub_menu_mut() {
                    Self::render_menu(ctx, menu, [pos[0] + ITEM_WIDTH - BORDER_SIZE, pos[1]]);
                }
            }
        }
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32) {
        if self.dirty {
            self.target.begin_frame();
            Self::render_menu(&mut self.target.ctx(), &mut self.menu, [0.0; 2]);
            self.target.end_frame(device);
            self.dirty = false;
        }

        let window_rect = Rect {
            translate: [self.pos[0], self.pos[1], depth],
            scale: self.target.size(),
        };
        ctx.draw_textured_rect(window_rect, self.target.shader_resource().clone());
    }
    fn intersect(&self, point: [f32; 2]) -> bool {
        false
    }
}

// only one menu open at a time,
// but there can be submenus
pub struct MenuManager {
    menu: Option<MenuView>,
}

impl MenuManager {
    pub fn new() -> MenuManager {
        MenuManager { menu: None }
    }
    pub fn open(&mut self, menu: MenuView) {
        self.menu = Some(menu);
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
