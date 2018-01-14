use super::{GuiElement, Model, Rect, RenderContext};
use super::render::*;

use gfx_device_gl as gl;
use glutin;

const ITEM_HEIGHT: f32 = 32.0;
const ITEM_WIDTH: f32 = 128.0;
const BORDER_SIZE: f32 = 1.0;
const HOVER_TIMEOUT: f32 = 0.25;

#[derive(Clone)]
pub struct MenuItem {
    label: String,
    sub_menu: Option<Menu>,
    hover: bool,
    hover_time: f32,
}

impl PartialEq for MenuItem {
    fn eq(&self, other: &MenuItem) -> bool {
        self.label == other.label && self.sub_menu == other.sub_menu && self.hover == other.hover
        // hover_time omitted
    }
}

impl MenuItem {
    fn new(label: String, sub_menu: Option<Menu>) -> MenuItem {
        MenuItem {
            label,
            sub_menu,
            hover: false,
            hover_time: 0.0,
        }
    }
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
    MenuItem::new(label.into(), None)
}
pub fn sub_menu(label: &str, items: &[MenuItem]) -> MenuItem {
    MenuItem::new(label.into(), Some(Menu::new(items)))
}

#[derive(Clone, PartialEq)]
pub struct Menu {
    items: Vec<MenuItem>,
    open: bool,
}

impl Menu {
    pub fn new(items: &[MenuItem]) -> Menu {
        Menu {
            items: items.into(),
            open: false,
        }
    }
    pub fn length(&self) -> usize {
        self.items
            .iter()
            .enumerate()
            .fold(self.items.len(), |acc, (idx, item)| {
                item.sub_menu()
                    .map(|child| acc.max(idx + child.length()))
                    .unwrap_or(acc)
            })
    }
    pub fn width(&self) -> usize {
        self.items.iter().fold(1, |acc, item| {
            item.sub_menu()
                .map(|child| acc.max(1 + child.width()))
                .unwrap_or(acc)
        })
    }
    pub fn any_children_hovered(&self) -> bool {
        self.items.iter().any(|item| {
            item.hover
                || item.sub_menu()
                    .map(|menu| menu.any_children_hovered())
                    .unwrap_or(false)
        })
    }
}

#[test]
fn test_menu() {
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
    assert_eq!(menu.length(), 6);
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
                color: if item.hover {
                    [0.0; 3]
                } else {
                    if item.sub_menu().map(|menu| menu.open).unwrap_or(false) {
                        [0.05; 3]
                    } else {
                        [0.1; 3]
                    }
                },
            });
            ctx.draw_text(&item.label(), [pos[0] + 4.0, pos[1] + 4.0], [1.0; 3]);

            if let Some(ref mut menu) = item.sub_menu_mut() {
                if menu.open {
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
    fn update_menu(model: &Model, menu: &mut Menu, offset: [f32; 2]) {
        for (idx, item) in menu.items.iter_mut().enumerate() {
            let pos = [
                offset[0],
                offset[1] + idx as f32 * (ITEM_HEIGHT - BORDER_SIZE),
            ];
            item.hover = point_in_rect(
                model.mouse_pos,
                &Rect {
                    translate: [pos[0], pos[1], 0.0],
                    scale: [ITEM_WIDTH - BORDER_SIZE, ITEM_HEIGHT - BORDER_SIZE],
                },
            );
            // Update last time mouse touched this item or submenu of it
            if (item.hover
                || item.sub_menu()
                    .map(|menu| menu.any_children_hovered())
                    .unwrap_or(false)) && item.sub_menu().is_some()
            {
                item.hover_time = model.time;
            }

            // Clear all submenu data (to be updated below if needed)
            if let Some(menu) = item.sub_menu_mut() {
                menu.open = false;
                for item in &mut menu.items {
                    item.hover = false;
                }
            }
        }
        // Find most recently hovered item with a submenu and update it
        // I sure hope time isn't ever NaN
        let sub_menu = menu.items
            .iter_mut()
            .enumerate()
            .max_by(|(_, item1), (_, item2)| item1.hover_time.partial_cmp(&item2.hover_time).unwrap());
        sub_menu.map(|(idx, item)| {
            let hover_time = item.hover_time;
            if let Some(menu) = item.sub_menu_mut() {
                if model.time - hover_time < HOVER_TIMEOUT || menu.any_children_hovered() {
                    let pos = [
                        offset[0],
                        offset[1] + idx as f32 * (ITEM_HEIGHT - BORDER_SIZE),
                    ];
                    Self::update_menu(model, menu, [pos[0] + ITEM_WIDTH - BORDER_SIZE, pos[1]]);
                    menu.open = true;
                }
            }
        });
    }
    fn update(&mut self, model: &Model) {
        let old_menu = self.menu.clone();
        Self::update_menu(model, &mut self.menu, self.pos);
        self.dirty |= self.menu != old_menu;
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
    fn update(&mut self, model: &Model) {
        self.menu.as_mut().map(|menu| menu.update(model));
    }
    fn handle(&mut self, model: &Model, event: &glutin::Event) {}
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton) {}
}