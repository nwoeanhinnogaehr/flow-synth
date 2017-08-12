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
                // get input
                let mut lock = node_ctx.lock();
                lock.wait(|x| x.available::<T>(InPortID(channel)) >= 1);
                let mut data = lock.read::<T>(InPortID(channel)).unwrap();
                drop(lock);

                // process
                let out: Vec<U> = data.drain(..).map(|x| f(x)).collect();

                // write output
                let mut lock = node_ctx.lock();
                lock.write(OutPortID(channel), &out).unwrap();
            }
        });
    }
}

pub struct NodeMap {
    pub id: NodeID,
    pub inputs: usize,
    pub outputs: usize,
}

impl NodeMap {
    pub fn new(graph: &mut Graph, inputs: usize, outputs: usize) -> NodeMap {
        NodeMap {
            id: graph.add_node(inputs, outputs),
            inputs,
            outputs,
        }
    }
    pub fn run<F, T, U>(self, ctx: &Context, f: F)
    where
        F: Fn(&[T]) -> Vec<U> + Send + 'static,
        T: ByteConvertible + Copy,
        U: ByteConvertible,
    {
        let node_ctx = ctx.node_ctx(self.id).unwrap();
        thread::spawn(move || loop {
            // get input
            let mut lock = node_ctx.lock();
            lock.wait(|x| (0..self.inputs).all(|id| x.available::<T>(InPortID(id)) >= 1));
            let data: Vec<T> = (0..self.inputs).map(|id| lock.read_n::<T>(InPortID(id), 1).unwrap()[0]).collect();
            drop(lock);

            // process
            let mut out: Vec<U> = f(&data);

            // write output
            let mut lock = node_ctx.lock();
            assert_eq!(out.len(), lock.node().out_ports().len());
            for (id, out) in out.drain(..).enumerate() {
                lock.write(OutPortID(id), &[out]).unwrap();
            }
        });
    }
}


// TODO LIST
//
// Mux/Demux
// Interleave/Deinterleave
// Fold/Split
// Filter/Multimap
// UnRLE (via Mutlimap?)
