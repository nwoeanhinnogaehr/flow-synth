#![feature(specialization)]
#![feature(plugin)]
#![feature(const_fn)]
#![feature(generators)]
#![feature(generator_trait)]
#![feature(libc)]
#![feature(drain_filter)]
#![feature(nll)]
#![feature(arbitrary_self_types)]
#![feature(never_type)]
#![feature(async_await)]
#![feature(atomic_min_max)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![deny(bare_trait_objects)]

extern crate crossbeam;
extern crate futures;
#[macro_use]
extern crate gfx;
extern crate cassowary;
extern crate gfx_device_gl;
extern crate gfx_glyph;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate jack;
extern crate ndarray;
extern crate nfd;
extern crate notify;
extern crate num;
extern crate ron;
extern crate serde;
#[macro_use]
extern crate serde_derive;

mod future_ext;
mod gui;
mod module;

fn main() {
    gui::gui_main();
}
