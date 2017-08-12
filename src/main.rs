extern crate modular_flow;
extern crate jack;

mod audio_io;
mod basics;

use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;

const CHANNELS: usize = 2;

fn main() {
    let mut graph = Graph::new();

    let audio_node = audio_io::AudioIONode::new(&mut graph, CHANNELS, CHANNELS);
    let process_node = basics::Map::new(&mut graph, CHANNELS);

    graph.connect_all(audio_node.id, process_node.id).unwrap();
    graph.connect_all(process_node.id, audio_node.id).unwrap();

    let ctx = Context::new(graph);

    audio_node.run(&ctx);
    process_node.run(&ctx, |x: f32| x.abs());

    thread::park();
}
