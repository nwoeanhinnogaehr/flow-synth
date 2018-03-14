#![feature(specialization)]
#![feature(plugin)]
#![feature(catch_expr)]
#![feature(fnbox)]
#![feature(const_fn)]
#![feature(universal_impl_trait)]
#![feature(conservative_impl_trait)]
#![feature(generators)]
#![feature(generator_trait)]
#![feature(match_default_bindings)]
#![feature(libc)]
#![feature(drain_filter)]
#![feature(nll)]
#![feature(arbitrary_self_types)]
#![feature(never_type)]
#![allow(dead_code)]
#![allow(unused_variables)]

extern crate crossbeam;
extern crate futures;
#[macro_use]
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_text;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate jack;
extern crate num;

mod module;
mod modular_flow;
mod gui;
mod future_ext;

fn main() {
    gui::gui_main();
}
