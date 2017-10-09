#![feature(specialization)]
#![feature(plugin)]
#![feature(catch_expr)]
#![feature(fnbox)]
#![plugin(rocket_codegen)]
#![feature(const_fn)]

extern crate apodize;
extern crate jack;
extern crate libloading;
extern crate modular_flow;
extern crate palette;
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate rocket_cors;
extern crate rustfft;
extern crate sdl2;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate ws;

mod audio_io;
//mod basics;
mod stft;
mod pixel_scroller;
mod web_api;
mod control;
mod serialize;
mod plugin_loader;

use std::env;

fn main() {
    println!("{:?}", plugin_loader::load("/home/i/flow-plugs/target/release/libflow_plugs.so"));
    let inst = if let Some(name) = env::args().nth(1) {
        serialize::from_file(&name)
    } else {
        control::Instance::new()
    };
    web_api::run_server(inst);
}
