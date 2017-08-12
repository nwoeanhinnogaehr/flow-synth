use modular_flow::graph::*;
use modular_flow::context::*;
use sdl2;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use std::thread;
use std::process;

pub struct Rainbowgram {
    pub id: NodeID,
    pub channels: usize,
}

impl Rainbowgram {
    pub fn new(graph: &mut Graph, channels: usize) -> Rainbowgram {
        Rainbowgram {
            id: graph.add_node(channels, 0),
            channels,
        }
    }
    pub fn run(self, context: &Context) {
        let node_ctx = context.node_ctx(self.id).unwrap();
        const fft_size: usize = 2048;
        const height: u32 = 1024;

        thread::spawn(move || {
            let sdl_context = sdl2::init().unwrap();
            let video_subsystem = sdl_context.video().unwrap();

            let window = video_subsystem.window("rainbowgram", fft_size as u32/2, height)
                .opengl()
                //.resizable()
                .build()
                .unwrap();

            let mut canvas = window.into_canvas().build().unwrap();
            let texture_creator = canvas.texture_creator();

            let mut texture = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, (fft_size/2) as u32, height)
                .unwrap();

            let mut event_pump = sdl_context.event_pump().unwrap();

            use rustfft::FFTplanner;
            use rustfft::num_complex::Complex;
            use rustfft::num_traits::Zero;

            let mut fft_input:  Vec<Complex<f32>> = vec![Complex::zero(); fft_size];
            let mut fft_output: Vec<Complex<f32>> = vec![Complex::zero(); fft_size];

            let mut planner = FFTplanner::new(false);
            let fft = planner.plan_fft(fft_size);

            let mut time = 0;

            'mainloop: loop {
                let mut lock = node_ctx.lock();
                lock.wait(|x| x.available::<f32>(InPortID(0)) >= fft_size);
                let data = lock.read_n::<f32>(InPortID(0), fft_size).unwrap();
                drop(lock);
                for i in 0..fft_size {
                    fft_input[i].re = data[i];
                }
                fft.process(&mut fft_input, &mut fft_output);

                for event in event_pump.poll_iter() {
                    match event {
                        Event::Quit {..} |
                            Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'mainloop,
                            Event::Window{win_event: WindowEvent::Resized(x, y), ..} => {
                                // TODO
                            }
                        _ => {}
                    }
                }

                let scroll_pos = time % (height as i32 / 2);
                time += 1;

                texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                    let y = scroll_pos as usize;
                    for x in 0..fft_size/2 {
                        let value = ((1.0 + fft_output[x].norm()).ln() * 128.0).min(255.0) as u8;
                        let offset = y * pitch + x * 3;
                        buffer[offset] = value;
                        buffer[offset+1] = value;
                        buffer[offset+2] = value;
                        let offset = (y+height as usize/2) * pitch + x * 3;
                        buffer[offset] = value;
                        buffer[offset+1] = value;
                        buffer[offset+2] = value;
                    }
                }).unwrap();

                canvas.clear();
                canvas.copy(&texture, Some(Rect::new(0, scroll_pos+1, (fft_size/2) as u32, height/2)), None).unwrap();
                canvas.present();

                //::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 60));
            }

            process::exit(0);
        });
    }
}
