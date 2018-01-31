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
#![feature(use_nested_groups)]
#![feature(libc)]
#![feature(drain_filter)]
#![allow(dead_code)]
#![allow(unused_variables)]

#[macro_use]
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_text;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate libc;
extern crate modular_flow;

mod module;
mod gui;

fn main() {
    gui::gui_main();
}
