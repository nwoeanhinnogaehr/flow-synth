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

                // process
                let out: Vec<U> = data.drain(..).map(|x| f(x)).collect();

                // write output
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

            // process
            let mut out: Vec<U> = f(&data);

            // write output
            assert_eq!(out.len(), lock.node().out_ports().len());
            for (id, out) in out.drain(..).enumerate() {
                lock.write(OutPortID(id), &[out]).unwrap();
            }
        });
    }
}

/**
 * A simple mapping from input to output ports. The given vector contains an element for each
 * output port, indicating the input port id to route from.
 */
pub struct PortIdxMap {
    pub id: NodeID,
    pub inputs: usize,
    pub outputs: usize,
    pub map: Vec<usize>
}

impl PortIdxMap {
    pub fn new(graph: &mut Graph, inputs: usize, outputs: usize, map: Vec<usize>) -> PortIdxMap {
        assert_eq!(map.len(), outputs);
        assert!(*map.iter().max().unwrap() < inputs);
        PortIdxMap {
            id: graph.add_node(inputs, outputs),
            inputs,
            outputs,
            map
        }
    }
    pub fn run(self, ctx: &Context) {
        let node_ctx = ctx.node_ctx(self.id).unwrap();
        thread::spawn(move || loop {
            // get input
            let mut lock = node_ctx.lock();
            // TODO we can ignore inputs that are unused in the map
            lock.wait(|x| (0..self.inputs).all(|id| x.available::<u8>(InPortID(id)) >= 1));
            let data: Vec<_> = (0..self.inputs).map(|id| lock.read::<u8>(InPortID(id)).unwrap()).collect();

            // write output
            for (dst, &src) in self.map.iter().enumerate() {
                lock.write(OutPortID(dst), &data[src]).unwrap();
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
