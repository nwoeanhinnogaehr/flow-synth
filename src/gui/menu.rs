use super::{GuiElement, Model, RenderContext};
use super::render::*;
use super::geom::*;

use std::sync::mpsc::{channel, Receiver, Sender};

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
    pos: Pt2,
    target: TextureTarget,
    dirty: bool,
    chan: Sender<MenuUpdate>,
}

impl MenuView {
    pub fn new(ctx: RenderContext, pos: Pt2, menu: Menu, chan: Sender<MenuUpdate>) -> MenuView {
        let size = Pt2::new(
            ITEM_WIDTH * menu.width() as f32,
            ITEM_HEIGHT * menu.length() as f32,
        );
        MenuView {
            menu,
            pos,
            target: TextureTarget::new(ctx, size),
            dirty: true,
            chan,
        }
    }
    fn render_menu(ctx: &mut RenderContext, menu: &mut Menu, offset: Pt2) {
        for (item, pos) in with_item_pos(menu.items.iter_mut(), offset) {
            // borders
            ctx.draw_rect(
                Rect2::new(pos, Pt2::new(ITEM_WIDTH, ITEM_HEIGHT)).upgrade(0.0),
                [1.0, 1.0, 1.0],
            );
            // background
            ctx.draw_rect(
                Rect2::new(
                    pos + BORDER_SIZE,
                    Pt2::new(
                        ITEM_WIDTH - BORDER_SIZE * 2.0,
                        ITEM_HEIGHT - BORDER_SIZE * 2.0,
                    ),
                ).upgrade(0.0),
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
            ctx.draw_text(&item.label(), pos + 4.0, [1.0; 3]);

            if let Some(ref mut menu) = item.sub_menu_mut() {
                if menu.open {
                    Self::render_menu(ctx, menu, pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0));
                }
            }
        }
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32) {
        if self.dirty {
            self.target.begin_frame();
            Self::render_menu(&mut self.target.ctx(), &mut self.menu, Pt2::zero());
            self.target.end_frame(device);
            self.dirty = false;
        }

        let window_rect = Rect2::new(self.pos, self.target.size()).upgrade(depth);
        ctx.draw_textured_rect(window_rect, self.target.shader_resource().clone());
    }
    fn intersect_impl<'a>(&self, menu: &'a Menu, offset: Pt2, path: &mut Vec<&'a str>) {
        for (item, pos) in with_item_pos(menu.items.iter(), offset) {
            if let Some(sub_menu) = item.sub_menu() {
                if sub_menu.open && sub_menu.any_children_hovered() || item.hover {
                    path.push(item.label());
                    self.intersect_impl(
                        sub_menu,
                        pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0),
                        path,
                    );
                }
            } else {
                if item.hover {
                    path.push(item.label());
                }
            }
        }
    }
    fn intersect(&self, mut offset: Pt2) -> Vec<&str> {
        let mut path = Vec::new();
        self.intersect_impl(&self.menu, offset, &mut path);
        path
    }
    fn update_menu(model: &Model, menu: &mut Menu, offset: Pt2) {
        for (item, pos) in with_item_pos(menu.items.iter_mut(), offset) {
            item.hover = Rect2::new(
                pos,
                Pt2::new(ITEM_WIDTH - BORDER_SIZE, ITEM_HEIGHT - BORDER_SIZE),
            ).intersect(model.mouse_pos);
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
        let sub_menu = with_item_pos(menu.items.iter_mut(), offset)
            .max_by(|(item1, _), (item2, _)| item1.hover_time.partial_cmp(&item2.hover_time).unwrap());
        sub_menu.map(|(item, pos)| {
            let hover_time = item.hover_time;
            if let Some(menu) = item.sub_menu_mut() {
                if model.time - hover_time < HOVER_TIMEOUT || menu.any_children_hovered() {
                    Self::update_menu(model, menu, pos + Pt2::new(ITEM_WIDTH - BORDER_SIZE, 0.0));
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

pub enum MenuUpdate {
    Select(Vec<String>),
    Abort,
}

// only one menu open at a time,
// but there can be submenus
pub struct MenuManager {
    menu: Option<MenuView>,
    ctx: RenderContext,
}

impl MenuManager {
    pub fn new(ctx: RenderContext) -> MenuManager {
        MenuManager { menu: None, ctx }
    }
    pub fn open(&mut self, menu: Menu, pos: Pt2) -> Receiver<MenuUpdate> {
        self.abort();
        let (tx, rx) = channel();
        self.menu = Some(MenuView::new(self.ctx.clone(), pos, menu, tx));
        rx
    }
    pub fn abort(&mut self) {
        if let Some(menu) = self.menu.take() {
            menu.chan.send(MenuUpdate::Abort);
        }
    }
}

impl GuiElement for MenuManager {
    fn set_pos(&mut self, pos: Pt2) {
        self.menu.as_mut().map(|menu| menu.pos = pos);
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext, depth: f32) {
        self.menu
            .as_mut()
            .map(|menu| menu.render(device, ctx, depth));
    }
    fn intersect(&self, point: Pt2) -> bool {
        self.menu
            .as_ref()
            .map(|menu| !menu.intersect(point).is_empty())
            .unwrap_or(false)
    }
    fn update(&mut self, model: &Model) {
        self.menu.as_mut().map(|menu| menu.update(model));
    }
    fn handle(&mut self, model: &Model, event: &glutin::Event) {}
    fn handle_click(&mut self, model: &Model, state: &glutin::ElementState, button: &glutin::MouseButton) {
        if *state == glutin::ElementState::Released && *button == glutin::MouseButton::Left {
            if let Some(menu) = self.menu.take() {
                let path = menu.intersect(model.mouse_pos);
                menu.chan
                    .send(MenuUpdate::Select(path.iter().map(|&x| x.into()).collect()));
            }
        }
    }
}
