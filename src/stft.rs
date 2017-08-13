use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;
use rustfft::{FFTnum, FFTplanner};
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use std::collections::VecDeque;
use std::iter;

pub fn run_stft<T: FFTnum + ByteConvertible>(ctx: NodeContext, size: usize, hop: usize)
where
    Complex<T>: ByteConvertible,
{
    thread::spawn(move || {
        let mut queue = VecDeque::<T>::new();
        queue.extend(vec![<T as Zero>::zero(); size - hop]);
        let mut input = vec![Complex::zero(); size];
        let mut output = vec![Complex::zero(); size];

        let mut planner = FFTplanner::new(false);
        let fft = planner.plan_fft(size);

        loop {
            for (in_port, out_port) in ctx.node().in_ports().iter().zip(ctx.node().out_ports()) {
                let lock = ctx.lock();
                lock.wait(|lock| lock.available::<T>(in_port.id()) >= hop);
                queue.extend(lock.read_n::<T>(in_port.id(), hop).unwrap());
                drop(lock);

                for (dst, src) in input.iter_mut().zip(queue.iter().cloned()) {
                    dst.re = src;
                    dst.im = Zero::zero();
                }
                queue.drain(0..hop);
                fft.process(&mut input, &mut output);

                DataFrame::write(&ctx.lock(), out_port.id(), &output[..output.len() / 2]);
            }
        }
    });
}

pub fn run_stft_render(ctx: NodeContext) {
    thread::spawn(move || loop {
        let lock = ctx.lock();
        let mut frame =
            lock.node().in_ports().iter().map(|port| DataFrame::<Complex<f32>>::read(&lock, port.id()));
        let ch1 = frame.next().unwrap();
        let frame = frame.skip(1).fold(ch1, |a, x| a.iter().zip(x.iter()).map(|(l, r)| l + r).collect());
        let out: Vec<_> = frame.iter().flat_map(|x| {
            let value = x.norm() * 32.0;
            let pixel = value as u8;
            iter::repeat(pixel).take(3)
        }).collect();
        DataFrame::write(&lock, OutPortID(0), &out);
    });
}
