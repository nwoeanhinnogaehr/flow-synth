use super::RenderContext;
use super::render::*;
use super::geom::*;
use super::component::*;
use super::event::*;

use gfx_device_gl as gl;

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

fn with_item_pos<T>(iter: impl Iterator<Item = T>, offset: Pt2) -> impl Iterator<Item = (T, Pt2)> {
    iter.enumerate().map(move |(idx, item)| {
        (
            item,
            offset + Pt2::new(0.0, idx as f32 * (ITEM_HEIGHT - BORDER_SIZE)),
        )
    })
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
    bounds: Box3,
    target: TextureTarget,
    dirty: bool,
}

impl MenuView {
    pub fn new(ctx: RenderContext, bounds: Box3, menu: Menu) -> MenuView {
        MenuView {
            menu,
            bounds,
            target: TextureTarget::new(ctx, bounds.size.drop_z()),
            dirty: true,
        }
    }
    fn render_menu(ctx: &mut RenderContext, menu: &mut Menu, offset: Pt2) {
        for (item, pos) in with_item_pos(menu.items.iter_mut(), offset) {
            // borders
            ctx.draw_rect(
                Rect3::new(pos.with_z(0.0), Pt2::new(ITEM_WIDTH, ITEM_HEIGHT)),
                [1.0, 1.0, 1.0],
            );
            // background
            ctx.draw_rect(
                Rect3::new(
                    (pos + BORDER_SIZE).with_z(0.0),
                    Pt2::new(
                        ITEM_WIDTH - BORDER_SIZE * 2.0,
                        ITEM_HEIGHT - BORDER_SIZE * 2.0,
                    ),
                ),
                if item.hover {
                    [0.0; 3]
                } else {
                    if item.sub_menu().map(|menu| menu.open).unwrap_or(false) {
                        [0.05; 3]
                    } else {
                        [0.1; 3]
                    }
                },
            );
            ctx.draw_text(&item.label(), (pos + 4.0).with_z(0.0), [1.0; 3]);

            if let Some(ref mut menu) = item.sub_menu_mut() {
                if menu.open {
                    Self::render_menu(ctx, menu, pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0));
                }
            }
        }
    }
    // Returns true if an item is hovered (false for a submenu)
    fn intersect_impl<'a>(&self, menu: &'a Menu, offset: Pt2, path: &mut Vec<&'a str>) -> bool {
        for (item, pos) in with_item_pos(menu.items.iter(), offset) {
            if let Some(sub_menu) = item.sub_menu() {
                if item.hover {
                    // on a submenu label
                    return false;
                }
                if sub_menu.open && sub_menu.any_children_hovered() {
                    path.push(item.label());
                    if !self.intersect_impl(
                        sub_menu,
                        pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0),
                        path,
                    ) {
                        return false;
                    }
                }
            } else {
                if item.hover {
                    path.push(item.label());
                }
            }
        }
        return true;
    }
    // returns Some([]) if a submenu is selected
    // or None if nothing is hit
    fn selection(&self, offset: Pt2) -> Option<Vec<&str>> {
        let mut path = Vec::new();
        if self.intersect_impl(&self.menu, offset, &mut path) {
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        } else {
            Some(Vec::new())
        }
    }
    fn update_menu(time: f32, mouse_pos: Pt2, menu: &mut Menu, offset: Pt2) {
        for (item, pos) in with_item_pos(menu.items.iter_mut(), offset) {
            item.hover = Rect2::new(pos, Pt2::new(ITEM_WIDTH, ITEM_HEIGHT)).intersect(mouse_pos);
            // Update last time mouse touched this item or submenu of it
            if (item.hover
                || item.sub_menu()
                    .map(|menu| menu.any_children_hovered())
                    .unwrap_or(false)) && item.sub_menu().is_some()
            {
                item.hover_time = time;
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
        let sub_menu = with_item_pos(menu.items.iter_mut(), offset)
            .max_by(|(item1, _), (item2, _)| item1.hover_time.partial_cmp(&item2.hover_time).unwrap());
        sub_menu.map(|(item, pos)| {
            let hover_time = item.hover_time;
            if let Some(menu) = item.sub_menu_mut() {
                if time - hover_time < HOVER_TIMEOUT || menu.any_children_hovered() {
                    Self::update_menu(
                        time,
                        mouse_pos,
                        menu,
                        pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0),
                    );
                    menu.open = true;
                }
            }
        });
    }
}

impl GuiComponent<MenuUpdate> for MenuView {
    fn bounds(&self) -> Box3 {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds = bounds;
    }
    fn intersect(&self, pos: Pt2) -> bool {
        self.selection(pos).is_some()
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        if self.dirty {
            self.target.begin_frame();
            Self::render_menu(&mut self.target.ctx(), &mut self.menu, Pt2::zero());
            self.target.end_frame(device);
            self.dirty = false;
        }

        ctx.draw_textured_rect(self.bounds.flatten(), self.target.shader_resource().clone());
    }
    fn handle(&mut self, event: &Event) -> MenuUpdate {
        match event.data {
            EventData::MouseMove(pos) => {
                let old_menu = self.menu.clone();
                Self::update_menu(event.time, pos, &mut self.menu, self.bounds.pos.drop_z());
                if self.menu != old_menu {
                    self.dirty = true;
                    MenuUpdate::NeedRender
                } else {
                    MenuUpdate::Unchanged
                }
            }
            EventData::Click(pos, button, state)
                if button == MouseButton::Left && state == ButtonState::Released =>
            {
                let path: Vec<_> = self.selection(pos)
                    .unwrap_or(Vec::new())
                    .iter()
                    .map(|&x| x.into())
                    .collect();
                if path.is_empty() {
                    MenuUpdate::Unchanged
                } else {
                    MenuUpdate::Select(path)
                }
            }
            _ => MenuUpdate::Unchanged,
        }
    }
}

pub enum MenuUpdate {
    Unchanged,
    NeedRender,
    Select(Vec<String>),
}
