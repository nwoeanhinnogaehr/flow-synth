extern crate modular_flow;
extern crate jack;
extern crate sdl2;
extern crate rustfft;

mod audio_io;
mod basics;
mod rainbowgram;

use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;

const CHANNELS: usize = 2;

fn main() {
    let mut graph = Graph::new();

    let audio = graph.add_node(CHANNELS, CHANNELS);
    let split = graph.add_node(CHANNELS, CHANNELS * 2);
    let process = graph.add_node(CHANNELS, CHANNELS);
    let delay = graph.add_node(CHANNELS, CHANNELS);
    let spectrogram = graph.add_node(CHANNELS, 0);

    graph.connect_all(audio, process).unwrap();
    graph.connect_all(process, delay).unwrap();
    graph.connect_all(delay, split).unwrap();
    graph.connect_n(split, OutPortID(0), audio, InPortID(0), CHANNELS).unwrap();
    graph.connect_n(split, OutPortID(CHANNELS), spectrogram, InPortID(0), CHANNELS).unwrap();

    let ctx = Context::new(graph);

    audio_io::run_audio_io(ctx.node_ctx(audio).unwrap());
    basics::run_map(ctx.node_ctx(process).unwrap(), |x: f32| x);
    basics::run_delay::<f32>(ctx.node_ctx(delay).unwrap(), 44100);
    basics::run_port_idx_map(
        ctx.node_ctx(split).unwrap(),
        (0..CHANNELS).cycle().take(CHANNELS * 2).collect(),
    );
    rainbowgram::run_spectrogram(ctx.node_ctx(spectrogram).unwrap());

    thread::park();
}
