#![feature(specialization)]
#![feature(plugin)]
#![feature(catch_expr)]
#![feature(fnbox)]
#![plugin(rocket_codegen)]
#![feature(const_fn)]
#![feature(libc)]

extern crate libloading;
extern crate modular_flow;
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;
extern crate rocket_cors;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate ws;
extern crate libc;

mod web_api;
mod control;
mod serialize;
mod plugin_loader;

use std::env;
use std::thread;

fn main() {
    let inst = env::args().nth(1).map(|name| serialize::from_file(&name)).unwrap_or(control::Instance::new());
    let id = env::args().nth(2).map(|id| id.parse().unwrap()).unwrap_or(0);
    thread::spawn(move ||{
        web_api::run_server(inst, id);

    });
    thread::park();
}
