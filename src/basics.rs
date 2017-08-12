use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;

pub struct Map {
    pub id: NodeID,
    pub channels: usize,
}

impl Map {
    pub fn new(graph: &mut Graph, channels: usize) -> Map {
        Map {
            id: graph.add_node(channels, channels),
            channels,
        }
    }
    pub fn run<F, T, U>(self, ctx: &Context, f: F)
    where
        F: Fn(T) -> U + Send + 'static,
        T: ByteConvertible,
        U: ByteConvertible,
    {
        let node_ctx = ctx.node_ctx(self.id).unwrap();
        thread::spawn(move || loop {
            // TODO don't require flow on all channels
            for channel in 0..self.channels {
                let mut lock = node_ctx.lock();
                lock.wait(|x| x.available::<T>(InPortID(channel)) >= 1);
                let mut data = lock.read::<T>(InPortID(channel)).unwrap();
                let out: Vec<U> = data.drain(..).map(|x| f(x)).collect();
                lock.write(OutPortID(channel), &out).unwrap();
            }
        });
    }
}

// TODO LIST
//
// MultiChannelMap
// Mux/Demux
// Interleave/Deinterleave
// Fold/Split
// Filter/Multimap
// UnRLE (via Mutlimap?)
