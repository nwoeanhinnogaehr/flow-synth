pub mod button;
pub mod component;
pub mod connect;
pub mod event;
pub mod geom;
pub mod layout;
pub mod menu;
pub mod module_gui;
pub mod render;
pub mod root;
pub mod textbox;

use self::component::*;
use self::event::*;
use self::geom::*;
use self::render::*;
use self::root::*;

use glutin::{self, ContextBuilder, WindowBuilder, EventsLoop};

use std::collections::VecDeque;
use std::time::Instant;

use gfx::Device;
use gfx_device_gl as gl;
use gfx_window_glutin as gfx_glutin;

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
            focus: true,
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
                Resized(pos) => {
                    self.window_size = Pt2::new(pos.width as f32, pos.height as f32);
                    self.root
                        .set_bounds(Box3::new(Pt3::zero(), self.window_size.with_z(0.0)));
                }
                CursorMoved {
                    device_id: _,
                    position,
                    modifiers: _,
                } => {
                    self.mouse_pos = Pt2::new((position.x as f32).floor(), (position.y as f32).floor());
                    self.generate_event(EventData::MouseMove(self.mouse_pos));
                }
                MouseInput {
                    device_id: _,
                    state,
                    button,
                    modifiers: _,
                } => {
                    self.generate_event(EventData::Click(self.mouse_pos, button.into(), state.into()));
                }
                KeyboardInput {
                    device_id: _,
                    input,
                } => {
                    if let Some(code) = input.virtual_keycode {
                        self.generate_event(EventData::Key(KeyEvent {
                            code,
                            modifiers: (&input.modifiers).into(),
                            state: (&input.state).into(),
                        }));
                    }
                }
                ReceivedCharacter(ch) => {
                    self.generate_event(EventData::Character(*ch));
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
       gfx_glutin::init::<ColorFormat, DepthFormat>(builder, context, &events_loop).unwrap();

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
                    CloseRequested => running = false,
                    Resized(..) => {
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
