use modular_flow::graph::*;
use modular_flow::context::*;
use sdl2;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use std::thread;
use std::process;
use std::mem;

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
            .create_texture_streaming(PixelFormatEnum::RGBA8888, width as u32, height as u32 * 2)
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

            let mut scroll_pos = 0;

            let lock = ctx.lock();
            lock.wait(|lock| lock.available::<u32>(InPortID(0)) >= width);
            let available_pixels = lock.available::<u32>(InPortID(0));
            if available_pixels >= width {
                let frames = lock.read_n::<u32>(InPortID(0), available_pixels / width * width).unwrap();
                drop(lock);

                for frame in frames.chunks(width) {
                    scroll_pos = time % (height as i32 / 2);
                    texture
                        .update(
                            Rect::new(0, scroll_pos, width as u32, 1),
                            unsafe { mem::transmute(&frame[..]) },
                            width * 4,
                        )
                        .unwrap();
                    texture
                        .update(
                            Rect::new(0, scroll_pos + height as i32 / 2, width as u32, 1),
                            unsafe { mem::transmute(&frame[..]) },
                            width * 4,
                        )
                        .unwrap();
                    time += 1;
                }
                canvas
                    .copy(&texture, Some(Rect::new(0, scroll_pos, width as u32, height as u32 / 2)), None)
                    .unwrap();
                canvas.present();
            }
        }

        process::exit(0);
    });
}
