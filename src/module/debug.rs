use futures::prelude::*;
use futures::executor;
use futures::future;

use module::{flow, Module};
use future_ext::Breaker;

use num::{One, Zero};
use std::ops::Add;

use std::sync::Arc;
use std::fmt::Debug;
use std::marker::PhantomData;

pub struct Printer<T: Debug + Send + Sync + 'static> {
    ifc: Arc<flow::Interface>,
    port: Arc<flow::Port<T, usize>>,
    breaker: Breaker,
    _t: PhantomData<T>,
}
impl<T: Debug + Send + Sync + 'static> Module for Printer<T> {
    fn new(ifc: Arc<flow::Interface>) -> Printer<T> {
        let port = ifc.add_port::<T, usize>("Input".into());
        Printer {
            ifc,
            port,
            breaker: Breaker::new(),
            _t: PhantomData,
        }
    }
    fn name() -> &'static str {
        "Printer"
    }
    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        exec.spawn(Box::new(future::loop_fn(
            (self.port.clone(), self.breaker.clone()),
            |(port, breaker)| {
                port.write1(1) // request 1 item
                .and_then(|port| port.read1()) // read the item
                .map(|(port, input)| {
                    println!("{:?}", input); // print it to console
                    port
                })
                .recover(|(port, err)| {
                    println!("PErr {:?}", err);
                    port
                })
                .map(|port| {
                    if breaker.test() {
                        future::Loop::Break(())
                    } else {
                        future::Loop::Continue((port, breaker))
                    }
                })
            },
        ))).unwrap();
    }
    fn stop(&mut self) {
        self.breaker.brake();
    }
    fn ports(&self) -> Vec<Arc<flow::OpaquePort>> {
        self.ifc.ports()
    }
}

pub struct Counter<T: Copy + One + Zero + Add + Send + 'static> {
    ifc: Arc<flow::Interface>,
    port: Arc<flow::Port<usize, T>>,
    breaker: Breaker,
    _t: PhantomData<T>,
}
impl<T: Copy + One + Zero + Add + Send + 'static> Module for Counter<T> {
    fn new(ifc: Arc<flow::Interface>) -> Counter<T> {
        let port = ifc.add_port::<usize, T>("Output".into());
        Counter {
            ifc,
            port,
            breaker: Breaker::new(),
            _t: PhantomData,
        }
    }
    fn name() -> &'static str {
        "Counter"
    }
    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        exec.spawn(Box::new(future::loop_fn(
            (self.port.clone(), T::zero(), self.breaker.clone()),
            |(port, mut count, breaker)| {
                port.read1() // read n
                    .and_then(move |(port, n)| {
                        // increment the current value n times, writing each value
                        port.write(
                            (0..n)
                                .map(|_| {
                                    count = count + T::one();
                                    count
                                })
                                .collect(),
                        ).map(move |port| (port, count)) // pass the new counter along
                    })
                    .recover(move |(data, err)| {
                        println!("CErr {:?}", err);
                        (data, count) // on error, reset back to the previous count (captured via move keyword)
                    })
                    .map(|(port, count)| {
                        if breaker.test() {
                            future::Loop::Break(())
                        } else {
                            future::Loop::Continue((port, count, breaker))
                        }
                    })
            },
        ))).unwrap();
    }
    fn stop(&mut self) {
        self.breaker.brake();
    }
    fn ports(&self) -> Vec<Arc<flow::OpaquePort>> {
        self.ifc.ports()
    }
}
