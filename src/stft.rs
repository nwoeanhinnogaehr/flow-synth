use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;
use rustfft::FFTplanner;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use std::collections::VecDeque;
use apodize;
use std::iter;
use super::control::*;
use std::sync::Arc;

pub struct Stft {}

impl NodeDescriptor for Stft {
    const NAME: &'static str = "STFT";
    fn new(ctx: Arc<Context>) -> Arc<RemoteControl> {
        let id = ctx.graph().add_node(0, 0);
        let node_ctx = ctx.node_ctx(id).unwrap();
        let node = ctx.graph().node(id);

        // TODO add ports for params
        let size = 2048;
        let hop = 256;

        let remote_ctl = Arc::new(RemoteControl::new(
            node,
            vec![
                message::Desc {
                    name: "Add port",
                    args: vec![],
                },
                message::Desc {
                    name: "Remove port",
                    args: vec![],
                },
            ],
        ));

        let ctl = remote_ctl.clone();
        let window: Vec<T> = apodize::hanning_iter(size).map(|x| x.sqrt() as T).collect();
        thread::spawn(move || {
            let mut empty_q = VecDeque::<T>::new();
            empty_q.extend(vec![0.0; size - hop]);
            let mut queues = vec![];
            let mut input = vec![Complex::zero(); size];
            let mut output = vec![Complex::zero(); size];

            let mut planner = FFTplanner::new(false);
            let fft = planner.plan_fft(size);

            loop {
                while let Some(msg) = ctl.recv_message() {
                    match msg.desc.name {
                        "Add port" => {
                            node_ctx.node().push_in_port();
                            node_ctx.node().push_out_port();
                            queues.push(empty_q.clone());
                        }
                        "Remove port" => {
                            node_ctx.node().pop_in_port();
                            node_ctx.node().pop_out_port();
                            queues.pop();
                        }
                        _ => panic!()
                    }
                }
                match ctl.poll() {
                    ControlState::Stopped => break,
                    ControlState::Paused => continue,
                    _ => (),
                }
                for ((in_port, out_port), queue) in
                    node_ctx.node().in_ports().iter().zip(node_ctx.node().out_ports()).zip(queues.iter_mut())
                {
                    let lock = node_ctx.lock();
                    lock.wait(|lock| lock.available::<T>(in_port.id()) >= hop);
                    queue.extend(lock.read_n::<T>(in_port.id(), hop).unwrap());
                    drop(lock);

                    for ((dst, src), mul) in input.iter_mut().zip(queue.iter()).zip(&window) {
                        dst.re = *src * mul;
                        dst.im = 0.0;
                    }
                    queue.drain(..hop);
                    fft.process(&mut input, &mut output);

                    node_ctx.lock().write(out_port.id(), &output[..output.len() / 2]).unwrap();
                }
            }
        });
        remote_ctl
    }
}

type T = f32; // fix this because generic numbers are so annoying

pub fn run_istft(ctx: NodeContext, size: usize, hop: usize) {
    let window: Vec<T> = apodize::hanning_iter(size).map(|x| x.sqrt() as T).collect();
    thread::spawn(move || {
        let mut queues = vec![
            {
                let mut q = VecDeque::<T>::new();
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
                lock.wait(|lock| lock.available::<Complex<T>>(in_port.id()) >= size / 2);
                let frame = lock.read_n::<Complex<T>>(in_port.id(), size / 2).unwrap();
                drop(lock);
                queue.extend(vec![0.0; hop]);
                let mut input: Vec<_> =
                    frame.iter().cloned().chain(iter::repeat(Complex::zero())).take(size).collect();
                fft.process(&mut input, &mut output);
                for ((src, dst), window) in output.iter().zip(queue.iter_mut()).zip(&window) {
                    *dst += src.re * *window / size as T / (size / hop) as T * 2.0;
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
    let mut prev_frame = vec![Complex::<T>::zero(); size];
    thread::spawn(move || loop {
        // TODO rewrite this so we can drop the lock during processing
        let lock = ctx.lock();
        let mut frame = lock.node().in_ports().into_iter().map(|port| {
            lock.wait(|lock| lock.available::<Complex<T>>(port.id()) >= size);
            lock.read_n::<Complex<T>>(port.id(), size).unwrap()
        });
        let ch1 = frame.next().unwrap();
        let frame = frame.fold(ch1, |a, x| a.iter().zip(x.iter()).map(|(l, r)| l + r).collect());
        let out: Vec<_> = (0..size)
            .map(|idx| {
                let x = (idx as T / size as T * (size as T).log2()).exp2();
                let x_i = x as usize;

                // compute hue
                let hue1 = frame[x_i].arg() - prev_frame[x_i].arg();
                let hue2 = frame[(x_i + 1) % size].arg() - prev_frame[(x_i + 1) % size].arg();

                // compute intensity
                let norm1 = frame[x_i].norm();
                let norm2 = frame[(x_i + 1) % size].norm();
                max = T::max(norm1, max);
                max = T::max(norm2, max);
                let value1 = norm1 / max;
                let value2 = norm2 / max;

                // output colour
                let grad = Gradient::new(vec![
                    Hsv::new(RgbHue::from_radians(hue1), 1.0, value1),
                    Hsv::new(RgbHue::from_radians(hue2), 1.0, value2),
                ]);
                let (r, g, b, _): (T, T, T, T) = Srgb::linear_to_pixel(grad.get(x % 1.0));
                let (r, g, b) = (r * 255.0, g * 255.0, b * 255.0);
                (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | 0xFF
            })
            .collect();
        prev_frame = frame;
        lock.write(OutPortID(0), &out).unwrap();
    });
}
