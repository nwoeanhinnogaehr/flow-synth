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

    let audio = audio_io::AudioIONode::new(&mut graph, CHANNELS, CHANNELS);
    let split = basics::PortIdxMap::new(&mut graph, CHANNELS, CHANNELS*2, (0..CHANNELS).cycle().take(CHANNELS*2).collect());
    let process = basics::Map::new(&mut graph, CHANNELS);
    let spectrogram = rainbowgram::Rainbowgram::new(&mut graph, CHANNELS);

    graph.connect_all(audio.id, process.id).unwrap();
    graph.connect_all(process.id, split.id).unwrap();
    graph.connect_n(split.id, OutPortID(0), audio.id, InPortID(0), CHANNELS).unwrap();
    graph.connect_n(split.id, OutPortID(CHANNELS), spectrogram.id, InPortID(0), CHANNELS).unwrap();

    let ctx = Context::new(graph);

    audio.run(&ctx);
    process.run(&ctx, |x: f32| x.abs());
    split.run(&ctx);
    spectrogram.run(&ctx);

    thread::park();
}
