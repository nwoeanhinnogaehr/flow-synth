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
            port.write(vec![1_usize])
            .and_then(|port| port.read_n::<T>(1))
            .map(|(port, input)| {
                println!("{:?}", input[0]);
                port
            })
            .recover(|(port, err)| {
                println!("PErr {:?}", err);
                port
            })
            .map(|port| future::Loop::Continue(port))
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
            port.read_n::<usize>(1)
            .and_then(move |(port, _)| port.write(vec![count]))
            .recover(|(port, err)| {
                println!("CErr {:?}", err);
                port
            })
            .map(move |port| future::Loop::Continue((count + T::one(), port)))
        }))).unwrap();
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}
