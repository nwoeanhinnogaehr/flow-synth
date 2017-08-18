use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;
use rustfft::FFTplanner;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use std::collections::VecDeque;
use apodize;
use std::iter;

type T = f32; // fix this because generic numbers are so annoying

pub fn run_stft(ctx: NodeContext, size: usize, hop: usize) -> usize {
    let window: Vec<T> = apodize::hanning_iter(size).map(|x| x as f32).collect();
    thread::spawn(move || {
        let mut queues = vec![
            {
                let mut q = VecDeque::<f32>::new();
                q.extend(vec![0.0; size - hop]);
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
                lock.wait(|lock| lock.available::<f32>(in_port.id()) >= hop);
                queue.extend(lock.read_n::<f32>(in_port.id(), hop).unwrap());
                drop(lock);

                for ((dst, src), mul) in input.iter_mut().zip(queue.iter()).zip(&window) {
                    dst.re = *src * mul;
                    dst.im = 0.0;
                }
                queue.drain(..hop);
                fft.process(&mut input, &mut output);

                ctx.lock().write(out_port.id(), &output[..output.len() / 2]).unwrap();
            }
        }
    });
    size / 2
}

pub fn run_istft(ctx: NodeContext, size: usize, hop: usize) {
    let window: Vec<f32> = apodize::hanning_iter(size).map(|x| x as f32).collect();
    thread::spawn(move || {
        let mut queues = vec![
            {
                let mut q = VecDeque::<f32>::new();
                q.extend(vec![0.0; size - hop]);
                q
            };
            ctx.node().in_ports().len()
        ];
        let mut output = vec![Complex::zero(); size];

        let mut planner = FFTplanner::new(true);
        let fft = planner.plan_fft(size);

        loop {
            for ((in_port, out_port), queue) in
                ctx.node().in_ports().iter().zip(ctx.node().out_ports()).zip(queues.iter_mut())
            {
                let lock = ctx.lock();
                lock.wait(|lock| lock.available::<Complex<f32>>(in_port.id()) >= size / 2);
                let frame = lock.read_n::<Complex<f32>>(in_port.id(), size / 2).unwrap();
                drop(lock);
                queue.extend(vec![0.0; hop]);
                let mut input: Vec<_> =
                    frame.iter().cloned().chain(iter::repeat(Complex::zero())).take(size).collect();
                fft.process(&mut input, &mut output);
                for ((src, dst), window) in output.iter().zip(queue.iter_mut()).zip(&window) {
                    *dst += src.re * *window / size as f32 / 4.0;
                }
                let samples = queue.drain(..hop).collect::<Vec<_>>();
                ctx.lock().write(out_port.id(), &samples).unwrap();
            }
        }
    });
}

pub fn run_stft_render(ctx: NodeContext, size: usize) {
    use palette::*;
    use palette::pixel::*;

    let mut max = 1.0;
    let mut prev_frame = vec![Complex::<f32>::zero(); size];
    thread::spawn(move || loop {
        // TODO rewrite this so we can drop the lock during processing
        let lock = ctx.lock();
        let mut frame = lock.node().in_ports().iter().map(|port| {
            lock.wait(|lock| lock.available::<Complex<f32>>(port.id()) >= size);
            lock.read_n::<Complex<f32>>(port.id(), size).unwrap()
        });
        let ch1 = frame.next().unwrap();
        let frame = frame.skip(1).fold(ch1, |a, x| a.iter().zip(x.iter()).map(|(l, r)| l + r).collect());
        let out: Vec<_> = frame
            .iter()
            .zip(prev_frame)
            .map(|(x, prev)| {
                // compute hue
                let hue = x.arg() - prev.arg();

                // compute intensity
                let norm = x.norm();
                max = f32::max(norm, max);
                let value = norm / max;

                // output colour
                let (r, g, b, a): (f32, f32, f32, f32) =
                    Srgb::linear_to_pixel(Hsv::new(RgbHue::from_radians(hue), 1.0, value));
                let (r, g, b, a) = (r * 255.0, g * 255.0, b * 255.0, a * 255.0);
                (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | (a as u32)
            })
            .collect();
        prev_frame = frame;
        lock.write(OutPortID(0), &out).unwrap();
    });
}
