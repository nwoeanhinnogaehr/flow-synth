extern crate modular_flow;
extern crate jack;
extern crate sdl2;
extern crate rustfft;
extern crate palette;
extern crate apodize;

mod audio_io;
mod basics;
mod stft;
mod pixel_scroller;

use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;

const CHANNELS: usize = 2;

fn main() {
    let mut graph = Graph::new();

    let audio = graph.add_node(CHANNELS, CHANNELS);
    let split = graph.add_node(CHANNELS, CHANNELS * 2);
    let fft = graph.add_node(CHANNELS, CHANNELS);
    let spectrogram = graph.add_node(CHANNELS, 1);
    let plotter = graph.add_node(1, 0);

    graph.connect_all(audio, split).unwrap();
    graph.connect_n(split, OutPortID(0), audio, InPortID(0), CHANNELS).unwrap();
    graph.connect_n(split, OutPortID(CHANNELS), fft, InPortID(0), CHANNELS).unwrap();
    graph.connect_all(fft, spectrogram).unwrap();
    graph.connect_all(spectrogram, plotter).unwrap();

    let ctx = Context::new(graph);

    audio_io::run_audio_io(ctx.node_ctx(audio).unwrap());
    basics::run_port_idx_map(
        ctx.node_ctx(split).unwrap(),
        (0..CHANNELS).cycle().take(CHANNELS * 2).collect(),
    );
    stft::run_stft(ctx.node_ctx(fft).unwrap(), 2048, 128);
    stft::run_stft_render(ctx.node_ctx(spectrogram).unwrap());
    pixel_scroller::run_pixel_scroller(ctx.node_ctx(plotter).unwrap(), 1024, 2048);

    thread::park();
}
