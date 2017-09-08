use conrod::{self, widget, Borderable, Colorable, Labelable, Positionable, Sizeable, Widget};
use conrod::backend::glium::glium::{self, Surface};
use conrod::color;
use modular_flow::context::*;
use modular_flow::graph::*;
use std::thread::{self, Thread, ThreadId};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process;
use std::sync::Arc;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::ops::{Deref, DerefMut};
use vec_map::VecMap;
use super::*;

pub trait NodeDescriptor {
    const NAME: &'static str;
    fn new(Arc<Context>) -> Box<GuiNode>;
}

pub trait GuiNode: Send + Sync {
    fn title(&self) -> String;
    fn run(&mut self) -> Arc<RemoteControl>;
    fn node(&self) -> &Node;
}

pub enum ControlState {
    Running,
    Paused,
    Stopped,
}
pub struct RemoteControl {
    pause_thread: Mutex<Option<Thread>>,
    stop_thread: Mutex<Option<Thread>>,
    paused: AtomicBool,
    stopped: AtomicBool,
}
impl RemoteControl {
    pub fn new() -> RemoteControl {
        RemoteControl {
            pause_thread: Mutex::new(None),
            stop_thread: Mutex::new(None),
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }
    }
    /**
     * Never returns `ControlState::Paused`, instead blocking until control is resumed.
     */
    pub fn poll_state_blocking(&self) -> ControlState {
        *self.pause_thread.lock().unwrap() = Some(thread::current());
        if self.stopped.load(Ordering::Acquire) {
            return ControlState::Stopped;
        }
        assert!(self.pause_thread.lock().unwrap().as_ref().unwrap().id() == thread::current().id());
        while self.paused.load(Ordering::Acquire) {
            thread::park();
        }
        ControlState::Running
    }
    pub fn block_until_stopped(&self) {
        *self.stop_thread.lock().unwrap() = Some(thread::current());
        while !self.stopped.load(Ordering::Acquire) {
            thread::park();
        }
    }
    pub fn poll(&self) -> ControlState {
        if self.stopped.load(Ordering::Acquire) {
            ControlState::Stopped
        } else if self.paused.load(Ordering::Acquire) {
            ControlState::Paused
        } else {
            ControlState::Running
        }
    }
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.pause_thread.lock().unwrap().as_ref().map(|thread| thread.unpark());
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.stop_thread.lock().unwrap().as_ref().map(|thread| thread.unpark());
    }
}

/*
struct ActiveNode<'a> {
    node: Box<GuiNode>,
    static_node: &'a StaticNodeListItem,
    ids: NodeIds,
}

struct StaticNodeListItem {
    name: &'static str,
    make: fn(Arc<Context>) -> Box<GuiNode>,
}
widget_ids!(struct Ids { canvas, left_bar, node_canvas, right_bar, left_bar_elem, node_canvas_elem, xypad });
widget_ids!(struct NodeIds { rect, label });

struct GuiState {
    static_idx: usize,
    active_idx: usize,
    mode: GuiMode,
}

impl GuiState {
    fn new() -> GuiState {
        GuiState {
            static_idx: 0,
            active_idx: 0,
            mode: GuiMode::StaticNormal,
        }
    }
}

#[derive(Copy, Clone)]
enum GuiMode {
    StaticNormal,
    ActiveNormal,
}

struct GuiInternals {
    ctx: Arc<Context>,
    node_ctx: NodeContext,
    state: RefCell<GuiState>,
}

impl GuiInternals {
    fn new(ctx: Arc<Context>) -> GuiInternals {
        let id = ctx.graph().add_node(0, 0);
        let node_ctx = ctx.node_ctx(id).unwrap();
        GuiInternals {
            ctx,
            node_ctx,
            state: RefCell::new(GuiState::new()),
        }
    }
}

pub struct Gui {
    internals: Rc<GuiInternals>,
}

impl NodeDescriptor for Gui {
    const NAME: &'static str = "GUI";
    fn new(ctx: Arc<Context>) -> Box<GuiNode> {
        Box::new(Gui {
            internals: Rc::new(GuiInternals::new(ctx)),
        })
    }
}

pub struct Seq<T> {
    thread_id: Cell<Option<ThreadId>>,
    inner: T,
}
impl<T> Seq<T> {
    pub fn new(inner: T) -> Seq<T> {
        Seq {
            thread_id: Cell::new(None),
            inner,
        }
    }
    fn test(&self) {
        if let Some(id) = self.thread_id.get() {
            assert!(id == thread::current().id());
        } else {
            self.thread_id.set(Some(thread::current().id()));
        }
    }
}
impl<T> Deref for Seq<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.test();
        &self.inner
    }
}
impl<T> DerefMut for Seq<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.test();
        &mut self.inner
    }
}
unsafe impl<T> Send for Seq<T> {}
unsafe impl<T> Sync for Seq<T> {}

impl GuiNode for Gui {
    fn title(&self) -> String {
        Gui::NAME.into()
    }
    fn run(&mut self) -> Arc<RemoteControl> {
        let remote_ctl = Arc::new(RemoteControl::new());
        let this = Seq::new(self.internals.clone());
        thread::spawn(move || {
            const WIDTH: u32 = 400;
            const HEIGHT: u32 = 200;

            // Build the window.
            let mut events_loop = glium::glutin::EventsLoop::new();
            let window_builder = glium::glutin::WindowBuilder::new()
                .with_title("Hello Conrod!")
                .with_dimensions(WIDTH, HEIGHT);
            let context = glium::glutin::ContextBuilder::new().with_vsync(true);
            let display = glium::Display::new(window_builder, context, &events_loop).unwrap();
            let this_window_id = display.gl_window().window().id();

            // construct our `Ui`.
            let mut ui = conrod::UiBuilder::new([WIDTH as f64, HEIGHT as f64]).build();

            let static_nodes = vec![
                StaticNodeListItem {
                    name: audio_io::AudioIO::NAME,
                    make: audio_io::AudioIO::new,
                },
                StaticNodeListItem {
                    name: Gui::NAME,
                    make: Gui::new,
                },
            ];
            // Generate the widget identifiers.
            let ids = Ids::new(ui.widget_id_generator());

            // Add a `Font` to the `Ui`'s `font::Map` from file.
            const FONT_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/Terminus.ttf");
            ui.fonts.insert_from_file(FONT_PATH).unwrap();

            // A type used for converting `conrod::render::Primitives` into `Command`s that can be used
            // for drawing to the glium `Surface`.
            let mut renderer = conrod::backend::glium::Renderer::new(&display).unwrap();

            // The image map describing each of our widget->image mappings (in our case, none).
            let image_map = conrod::image::Map::<glium::texture::Texture2d>::new();

            let mut active_nodes = Vec::new();

            events_loop.run_forever(|event| {
                let mut state = this.state.borrow_mut();

                // Break from the loop upon `Escape` or closed window.
                match event.clone() {
                    glium::glutin::Event::WindowEvent { event, window_id } => {
                        if window_id != this_window_id {
                            return glium::glutin::ControlFlow::Continue;
                        }
                        match event {
                            glium::glutin::WindowEvent::Closed => return glium::glutin::ControlFlow::Break,
                            glium::glutin::WindowEvent::KeyboardInput {
                                input: glium::glutin::KeyboardInput {
                                    virtual_keycode: Some(key),
                                    state: glium::glutin::ElementState::Pressed,
                                    ..
                                },
                                ..
                            } => match key {
                                glium::glutin::VirtualKeyCode::Escape => {
                                    return glium::glutin::ControlFlow::Break
                                }
                                glium::glutin::VirtualKeyCode::J => match state.mode {
                                    GuiMode::StaticNormal if state.static_idx < static_nodes.len() - 1 => {
                                        state.static_idx += 1
                                    }
                                    GuiMode::ActiveNormal if state.active_idx < active_nodes.len() - 1 => {
                                        state.active_idx += 1
                                    }
                                    _ => (),
                                },
                                glium::glutin::VirtualKeyCode::K => match state.mode {
                                    GuiMode::StaticNormal if state.static_idx > 0 => state.static_idx -= 1,
                                    GuiMode::ActiveNormal if state.active_idx > 0 => state.active_idx -= 1,
                                    _ => (),
                                },
                                glium::glutin::VirtualKeyCode::H => match state.mode {
                                    GuiMode::ActiveNormal if static_nodes.len() > 0 => {
                                        state.mode = GuiMode::StaticNormal
                                    }
                                    _ => (),
                                },
                                glium::glutin::VirtualKeyCode::L => match state.mode {
                                    GuiMode::StaticNormal if active_nodes.len() > 0 => {
                                        state.mode = GuiMode::ActiveNormal
                                    }
                                    _ => (),
                                },
                                _ => (),
                            },
                            _ => (),
                        }
                    }
                    _ => (),
                }
                // Use the `winit` backend feature to convert the winit event to a conrod one.
                let input = match conrod::backend::winit::convert_event(event, &display) {
                    None => return glium::glutin::ControlFlow::Continue,
                    Some(input) => input,
                };

                // Handle the input with the `Ui`.
                ui.handle_event(input);

                // Set the widgets.
                {
                    const ITEM_SIZE: f64 = 50.0;

                    let ui = &mut ui.set_widgets();

                    let canvas = widget::Canvas::new()
                        .flow_right(&[
                            (ids.left_bar, widget::Canvas::new()),
                            (ids.node_canvas, widget::Canvas::new().color(color::BLACK)),
                            (ids.right_bar, widget::Canvas::new()),
                        ])
                        .set(ids.canvas, ui);

                    let (mut left_bar_elem, left_bar_scrollbar) = widget::List::flow_down(static_nodes.len())
                        .item_size(ITEM_SIZE)
                        .scrollbar_next_to()
                        .scrollbar_color(color::LIGHT_BLUE)
                        .middle_of(ids.left_bar)
                        .wh_of(ids.left_bar)
                        .set(ids.left_bar_elem, ui);
                    while let Some(elem) = left_bar_elem.next(ui) {
                        let node = &static_nodes[elem.i];
                        let mut button = widget::Button::new()
                            .label(node.name)
                            .label_font_size(13)
                            .border(4.0)
                            .top_left_of(ids.canvas);
                        if let GuiMode::StaticNormal = state.mode {
                            if state.static_idx == elem.i {
                                button = button.border_color(color::LIGHT_RED);
                            }
                        }
                        for _click in elem.set(button, ui) {
                            let mut guinode = (node.make)(this.ctx.clone());
                            guinode.run();
                            active_nodes.push(ActiveNode {
                                node: guinode,
                                static_node: node,
                                ids: NodeIds::new(ui.widget_id_generator()),
                            });
                        }
                    }

                    let (mut node_canvas_elem, node_canvas_scrollbar) = widget::List::flow_down(
                        active_nodes.len(),
                    ).item_size(ITEM_SIZE)
                        .scrollbar_next_to()
                        .scrollbar_color(color::LIGHT_BLUE)
                        .middle_of(ids.node_canvas)
                        .wh_of(ids.node_canvas)
                        .set(ids.node_canvas_elem, ui);
                    if let Some(s) = node_canvas_scrollbar {
                        s.set(ui)
                    }
                    while let Some(elem) = node_canvas_elem.next(ui) {
                        let node = &active_nodes[elem.i];
                        let canvas = widget::Canvas::new().wh_of(ids.node_canvas);
                        elem.set(canvas, ui);
                        let mut rect = widget::BorderedRectangle::new(
                            [ui.wh_of(ids.node_canvas).unwrap()[0], ITEM_SIZE],
                        ).border(4.0)
                            .middle_of(elem.widget_id);
                        if let GuiMode::ActiveNormal = state.mode {
                            if state.active_idx == elem.i {
                                rect = rect.border_color(color::LIGHT_RED);
                            }
                        }
                        rect.set(node.ids.rect, ui);
                        let n_in = node.node.node().in_ports().len();
                        let n_out = node.node.node().out_ports().len();
                        let n_con_in =
                            node.node.node().in_ports().iter().filter(|x| x.edge().is_some()).count();
                        let n_con_out =
                            node.node.node().out_ports().iter().filter(|x| x.edge().is_some()).count();
                        let text = format!(
                            "[{}; >{}/{}; <{}/{}] {} - {}",
                            (elem.i as u8 + 'A' as u8) as char,
                            n_con_in,
                            n_in,
                            n_con_out,
                            n_out,
                            node.static_node.name,
                            node.node.title()
                        );
                        widget::Text::new(&text)
                            .font_size(13)
                            .middle_of(elem.widget_id)
                            .set(node.ids.label, ui);
                    }

                    if let Some((x, y)) = widget::xy_pad::XYPad::new(0.0, -1.0, 1.0, 0.0, -1.0, 1.0)
                      .w_h(300f64, 300f64)
                      .set(ids.xypad, ui)
                      {
                      let lock = node_ctx.lock();
                      lock.write(OutPortID(0), &[(x, y)]);
                      }
                }

                // Draw the `Ui` if it has changed.
                if let Some(primitives) = ui.draw_if_changed() {
                    renderer.fill(&display, primitives, &image_map);
                    let mut target = display.draw();
                    target.clear_color(0.0, 0.0, 0.0, 1.0);
                    renderer.draw(&display, &mut target, &image_map).unwrap();
                    target.finish().unwrap();
                }


                glium::glutin::ControlFlow::Continue
            });

            process::exit(0);
        });
        remote_ctl
    }
    fn node(&self) -> &Node {
        self.internals.node_ctx.node()
    }
}
*/
