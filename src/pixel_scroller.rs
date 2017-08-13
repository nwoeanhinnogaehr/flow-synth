use modular_flow::graph::*;
use modular_flow::context::*;
use sdl2;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use std::thread;
use std::process;

pub fn run_pixel_scroller(ctx: NodeContext, width: usize, height: usize) {
    assert_eq!(ctx.node().in_ports().len(), 1); // we support only 1 input
    assert_eq!(ctx.node().out_ports().len(), 0); // we output only to the screen

    thread::spawn(move || {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();

        let window = video_subsystem
            .window("pixel-scroller", width as u32, height as u32)
            .opengl()
            .resizable()
            .build()
            .unwrap();

        let mut canvas = window.into_canvas().build().unwrap();
        let texture_creator = canvas.texture_creator();

        let mut texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB24, width as u32, height as u32 * 2)
            .unwrap();

        let mut event_pump = sdl_context.event_pump().unwrap();

        let mut time = 0;

        'mainloop: loop {
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } |
                    Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => break 'mainloop,
                    Event::Window {
                        win_event: WindowEvent::Resized(_, _),
                        ..
                    } => {
                        // TODO
                    }
                    _ => {}
                }
            }

            let scroll_pos = time % (height as i32 / 2);
            time += 1;

            let lock = ctx.lock();
            let frame = DataFrame::<u8>::read(&lock, InPortID(0));
            assert_eq!(frame.len(), width);
            drop(lock);

            let fill_fn = |buffer: &mut [u8], _: usize| for x in 0..width {
                let value = frame[x];
                let offset = x * 3;
                buffer[offset] = value;
                buffer[offset + 1] = value;
                buffer[offset + 2] = value;
            };
            texture.with_lock(Rect::new(0, scroll_pos, width as u32, 1), &fill_fn).unwrap();
            texture
                .with_lock(Rect::new(0, scroll_pos + height as i32 / 2, width as u32, 1), &fill_fn)
                .unwrap();

            canvas.clear();
            canvas
                .copy(&texture, Some(Rect::new(0, scroll_pos + 1, width as u32, height as u32 / 2)), None)
                .unwrap();
            canvas.present();

            //::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
        }

        process::exit(0);
    });
}
