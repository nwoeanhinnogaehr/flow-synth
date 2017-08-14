use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;
use rustfft::FFTplanner;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use std::collections::VecDeque;
use apodize;

type T = f32; // fix this because generic numbers are so annoying

pub fn run_stft(ctx: NodeContext, size: usize, hop: usize) {
    let window: Vec<T> = apodize::hanning_iter(size).map(|x| x.sqrt() as T).collect();
    thread::spawn(move || {
        let mut queues = vec![
            {
                let mut q = VecDeque::<T>::new();
                q.extend(vec![<T as Zero>::zero(); size - hop]);
                q
            };
            ctx.node().in_ports().len()
        ];
        let mut input = vec![Complex::zero(); size];
        let mut output = vec![Complex::zero(); size];

        let mut planner = FFTplanner::new(false);
        let fft = planner.plan_fft(size);

        loop {
            for ((in_port, out_port), queue) in
                ctx.node().in_ports().iter().zip(ctx.node().out_ports()).zip(queues.iter_mut())
            {
                let lock = ctx.lock();
                lock.wait(|lock| lock.available::<T>(in_port.id()) >= hop);
                queue.extend(lock.read_n::<T>(in_port.id(), hop).unwrap());
                drop(lock);

                for ((dst, src), mul) in input.iter_mut().zip(queue.iter()).zip(&window) {
                    dst.re = *src * mul;
                    dst.im = 0.0;
                }
                queue.drain(..hop);
                fft.process(&mut input, &mut output);

                DataFrame::write(&ctx.lock(), out_port.id(), &output[..output.len() / 2]);
            }
        }
    });
}

pub fn run_stft_render(ctx: NodeContext) {
    let mut max = 1.0;
    use palette::*;
    use palette::pixel::*;
    thread::spawn(move || loop {
        let lock = ctx.lock();
        let mut frame =
            lock.node().in_ports().iter().map(|port| DataFrame::<Complex<f32>>::read(&lock, port.id()));
        let ch1 = frame.next().unwrap();
        let frame = frame.skip(1).fold(ch1, |a, x| a.iter().zip(x.iter()).map(|(l, r)| l + r).collect());
        let out: Vec<_> = frame
            .iter()
            .map(|x| {
                let norm = x.norm();
                max = f32::max(norm, max);
                let hue = x.arg();
                let value = norm / max;
                let (r, g, b, a): (f32, f32, f32, f32) =
                    Srgb::linear_to_pixel(Hsv::new(RgbHue::from_radians(hue), 1.0, value));
                let (r, g, b, a) = (r * 255.0, g * 255.0, b * 255.0, a * 255.0);
                (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | (a as u32)
            })
            .collect();
        DataFrame::write(&lock, OutPortID(0), &out);
    });
}
