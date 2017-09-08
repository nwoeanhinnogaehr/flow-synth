#![feature(specialization)]
#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate apodize;
#[macro_use]
extern crate conrod;
extern crate jack;
extern crate modular_flow;
extern crate palette;
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate rocket_cors;
extern crate rustfft;
extern crate sdl2;
extern crate vec_map;

mod audio_io;
mod basics;
mod stft;
mod pixel_scroller;
mod gui;
mod wapi;

use modular_flow::graph::*;
use modular_flow::context::*;
use gui::*;
use std::thread;
use std::sync::Arc;

fn main() {
    let ctx = Arc::new(Context::new(Graph::new()));
    //Gui::new(ctx).run();
    //loop {
    //thread::park();
    //}
    wapi::run_server(ctx);
}
