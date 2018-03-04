use futures::prelude::*;
use futures::executor;
use futures::future;

use module::Module;
use modular_flow as mf;

use num::{One, Zero};
use std::ops::Add;

use std::sync::Arc;
use std::fmt::Debug;
use std::marker::PhantomData;

pub struct Printer<T: Debug + Send + Sync + 'static> {
    ifc: Arc<mf::Interface>,
    port: Arc<mf::Port>,
    _t: PhantomData<T>,
}
impl<T: Debug + Send + Sync + 'static> Module for Printer<T> {
    fn new(ifc: Arc<mf::Interface>) -> Printer<T> {
        let port = ifc.add_port(&mf::MetaPort::new::<T, usize, _>("Input"));
        Printer {
            ifc,
            port,
            _t: PhantomData,
        }
    }
    fn name() -> &'static str {
        "Printer"
    }
    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        exec.spawn(Box::new(future::loop_fn(self.port.clone(), |port| {
            let p2 = port.clone();
            port.write(vec![1_usize])
            .and_then(move |_| p2.read_n(1))
            .map(|input: Box<[T]>| println!("{:?}", input[0]))
            .then(move |result| {
                if let Err(e) = result {
                    println!("result {:?}", e);
                }
                Ok(future::Loop::Continue(port))
            })
        }))).unwrap();
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}

pub struct Counter<T: Copy + One + Zero + Add + Send + 'static> {
    ifc: Arc<mf::Interface>,
    port: Arc<mf::Port>,
    _t: PhantomData<T>,
}
impl<T: Copy + One + Zero + Add + Send + 'static> Module for Counter<T> {
    fn new(ifc: Arc<mf::Interface>) -> Counter<T> {
        let port = ifc.add_port(&mf::MetaPort::new::<usize, T, _>("Output"));
        Counter {
            ifc,
            port,
            _t: PhantomData,
        }
    }
    fn name() -> &'static str {
        "Counter"
    }
    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        exec.spawn(Box::new(future::loop_fn((T::zero(), self.port.clone()), |(count, port)| {
            let p2 = port.clone();
            port.read_n(1)
            .and_then(move |x: Box<[usize]>| p2.write(vec![count]))
            .then(move |result| {
                if let Err(e) = result {
                    println!("result {:?}", e);
                }
                Ok(future::Loop::Continue((count + T::one(), port)))
            })
        }))).unwrap();
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}
