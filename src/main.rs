#![feature(specialization)]
#![feature(plugin)]
#![feature(catch_expr)]
#![feature(fnbox)]
#![plugin(rocket_codegen)]

extern crate apodize;
extern crate jack;
extern crate modular_flow;
extern crate palette;
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate rocket_cors;
extern crate rustfft;
extern crate sdl2;
extern crate ws;

mod audio_io;
//mod basics;
mod stft;
mod pixel_scroller;
mod web_api;
mod control;

use modular_flow::graph::*;
use modular_flow::context::*;
use std::sync::Arc;

fn main() {
    let ctx = Arc::new(Context::new(Graph::new()));
    web_api::run_server(ctx);
}
