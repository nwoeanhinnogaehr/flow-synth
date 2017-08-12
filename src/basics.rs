use modular_flow::graph::*;
use modular_flow::context::*;
use std::thread;

pub fn run_map<F, T, U>(ctx: NodeContext, f: F)
where
    F: Fn(T) -> U + Send + 'static,
    T: ByteConvertible,
    U: ByteConvertible,
{
    thread::spawn(move || loop {
        // TODO don't require flow on all channels
        for (in_port, out_port) in ctx.node().in_ports().iter().zip(ctx.node().out_ports()) {
            // get input
            let lock = ctx.lock();
            lock.wait(|lock| lock.available::<T>(in_port.id()) >= 1);
            let mut data = lock.read::<T>(in_port.id()).unwrap();

            // process
            let out: Vec<U> = data.drain(..).map(|x| f(x)).collect();

            // write output
            lock.write(out_port.id(), &out).unwrap();
        }
    });
}

pub fn run_node_map<F, T, U>(ctx: NodeContext, f: F)
where
    F: Fn(&[T]) -> Vec<U> + Send + 'static,
    T: ByteConvertible + Copy,
    U: ByteConvertible,
{
    thread::spawn(move || loop {
        // get input
        let lock = ctx.lock();
        lock.wait(|lock| lock.node().in_ports().iter().all(|port| lock.available::<T>(port.id()) >= 1));
        let data: Vec<T> =
            lock.node().in_ports().iter().map(|port| lock.read_n::<T>(port.id(), 1).unwrap()[0]).collect();

        // process
        let mut out: Vec<U> = f(&data);

        // write output
        for (out_data, out_port) in out.drain(..).zip(lock.node().out_ports()) {
            lock.write(out_port.id(), &[out_data]).unwrap();
        }
    });
}

/**
 * A simple mapping from input to output ports. The given vector contains an element for each
 * output port, indicating the input port id to route from.
 */
pub fn run_port_idx_map(ctx: NodeContext, map: Vec<usize>) {
    assert_eq!(map.len(), ctx.node().out_ports().len());
    assert!(*map.iter().max().unwrap() < ctx.node().in_ports().len());
    thread::spawn(move || loop {
        // get input
        let lock = ctx.lock();
        // TODO we can ignore inputs that are unused in the map
        lock.wait(|lock| lock.node().in_ports().iter().all(|port| lock.available::<u8>(port.id()) >= 1));
        let data: Vec<_> =
            lock.node().in_ports().iter().map(|port| lock.read::<u8>(port.id()).unwrap()).collect();

        // write output
        for (dst, &src) in map.iter().enumerate() {
            lock.write(OutPortID(dst), &data[src]).unwrap();
        }
    });
}

/**
 * Copies all data from input ports to corresponding output ports.
 * If there are more of one type of port than the other, the extras will be ignored.
 */
pub fn run_identity(ctx: NodeContext) {
    thread::spawn(move || loop {
        let lock = ctx.lock();
        for (in_data, out_port) in ctx.node()
            .in_ports()
            .iter()
            .inspect(|port| lock.wait(|lock| lock.available::<u8>(port.id()) >= 1))
            .map(|port| lock.read::<u8>(port.id()).unwrap())
            .zip(ctx.node().out_ports().iter())
        {
            lock.write(out_port.id(), &in_data).unwrap();
        }
    });
}

// i'm realizing now that this is really a half-assed delay line,
// because it does very little delaying when the consumer is greedy
pub fn run_delay<T: Default + Clone + ByteConvertible>(ctx: NodeContext, delay: usize) {
    let nulls = vec![T::default(); delay];
    for port in ctx.node().out_ports() {
        ctx.lock().write::<T>(port.id(), &nulls).unwrap();
    }
    run_identity(ctx);
}

// TODO LIST
//
// Mux/Demux
// Interleave/Deinterleave
// Fold/Split
// Filter/Multimap
// UnRLE (via Mutlimap?)
