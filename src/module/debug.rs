use super::Module;

use modular_flow as mf;

use num::{One, Zero};
use std::ops::Add;

use std::sync::Arc;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::thread;

pub struct Printer<T: Debug + 'static> {
    ifc: Arc<mf::Interface>,
    port: Arc<mf::Port>,
    _t: PhantomData<T>,
}
impl<T: Debug + 'static> Module for Printer<T> {
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
    fn start(&mut self) {
        let port = self.port.clone();
        let ifc = self.ifc.clone();
        thread::spawn(move || loop {
            ifc.write(&port, vec![1_usize]);
            let sig = ifc.wait(&port);
            if sig == mf::Signal::Write {
                if let Ok(data) = ifc.read_n::<T>(&port, 1) {
                    println!("{:?}", data[0]);
                }
            }
        });
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}

pub struct Counter<T: Copy + One + Zero + Add + 'static> {
    ifc: Arc<mf::Interface>,
    port: Arc<mf::Port>,
    _t: PhantomData<T>,
}
impl<T: Copy + One + Zero + Add + 'static> Module for Counter<T> {
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
    fn start(&mut self) {
        let port = self.port.clone();
        let ifc = self.ifc.clone();
        thread::spawn(move || {
            let mut count = T::zero();
            loop {
                let sig = ifc.wait(&port);
                if sig == mf::Signal::Write {
                    if let Ok(n) = ifc.read_n::<usize>(&port, 1) {
                        ifc.write::<T, _>(
                            &port,
                            (0..n[0])
                                .map(|i| {
                                    count = count + T::one();
                                    count
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
            }
        });
    }
    fn ports(&self) -> Vec<Arc<mf::Port>> {
        self.ifc.ports()
    }
}
