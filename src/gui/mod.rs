mod render;
mod menu;
mod button;
mod geom;
mod component;
mod event;
mod module_gui;
mod root;

use self::render::*;
use self::geom::*;
use self::event::*;
use self::root::*;
use self::component::*;

use glutin::{self, ContextBuilder, EventsLoop, GlContext, WindowBuilder};

use std::time::Instant;
use std::collections::VecDeque;

use gfx::Device;
use gfx_window_glutin as gfx_glutin;
use gfx_device_gl as gl;

/// Model holds info about GUI state
/// and program state
pub struct Model {
    ctx: RenderContext,
    time: f32,
    window_size: Pt2,
    mouse_pos: Pt2,
    root: Root,
}

impl Model {
    fn new(ctx: RenderContext) -> Model {
        Model {
            time: 0.0,
            window_size: Pt2::zero(),
            mouse_pos: Pt2::zero(),
            root: Root::new(ctx.clone(), Box3::new(Pt3::zero(), Pt3::zero())),
            ctx,
        }
    }

    fn generate_event(&mut self, data: EventData) {
        self.root.handle(&Event {
            time: self.time,
            data,
            scope: Scope::Local,
        });
        self.root.handle(&Event {
            time: self.time,
            data,
            scope: Scope::Global,
        });
    }

    fn handle(&mut self, event: &glutin::Event) {
        use glutin::WindowEvent::*;
        //println!("{:?}", event);
        match event {
            glutin::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                Resized(w, h) => {
                    self.window_size = Pt2::new(*w as f32, *h as f32);
                    self.root
                        .set_bounds(Box3::new(Pt3::zero(), self.window_size.with_z(0.0)));
                }
                CursorMoved {
                    device_id: _,
                    position,
                    modifiers: _,
                } => {
                    self.mouse_pos = Pt2::new((position.0 as f32).floor(), (position.1 as f32).floor());
                    self.generate_event(EventData::MouseMove(self.mouse_pos));
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                    modifiers: _,
                } => {
                    self.generate_event(EventData::Click(
                        self.mouse_pos,
                        button.into(),
                        state.into(),
                    ));
                }
                _ => (),
            },
            _ => (),
        }
    }

    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        self.root.render(device, ctx);
    }
}

pub fn gui_main() {
    // init window
    let mut events_loop = EventsLoop::new();
    let context = ContextBuilder::new().with_gl_profile(glutin::GlProfile::Core);
    let builder = WindowBuilder::new().with_title(String::from("flow-synth"));
    let (window, mut device, factory, main_color, main_depth) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop);

    let mut target = Target {
        color: main_color,
        depth: main_depth,
    };

    // init rendering pipeline
    let mut ctx = RenderContext::new(factory.clone());

    let mut model = Model::new(ctx.clone());

    // begin main loop
    let mut running = true;
    let timer = Instant::now();
    let mut frames = VecDeque::new();
    loop {
        // update fps
        let now = timer.elapsed();
        model.time = now.as_secs() as f32 + now.subsec_nanos() as f32 / 1_000_000_000.0;
        frames.push_back(model.time);
        while let Some(&old_frame) = frames.front() {
            if old_frame < model.time - 1.0 {
                frames.pop_front();
            } else {
                break;
            }
        }

        // handle events
        events_loop.poll_events(|event| {
            model.handle(&event);
            use glutin::WindowEvent::*;
            match event {
                glutin::Event::WindowEvent {
                    window_id: _,
                    event,
                } => match event {
                    Closed => running = false,
                    Resized(_, _) => {
                        gfx_glutin::update_views(&window, &mut target.color, &mut target.depth);
                    }
                    _ => (),
                },
                _ => (),
            }
        });

        if !running {
            break;
        }

        ctx.begin_frame(&target);

        model.render(&mut device, &mut ctx);

        // debug text
        ctx.draw_text(
            &format!("FPS={} Time={}", frames.len(), model.time),
            Pt3::new(0.0, 0.0, 0.0),
            [1.0, 1.0, 1.0],
        );

        ctx.end_frame(&mut device, &target);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
