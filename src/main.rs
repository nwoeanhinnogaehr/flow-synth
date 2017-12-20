#![feature(specialization)]
#![feature(plugin)]
#![feature(catch_expr)]
#![feature(fnbox)]
#![plugin(rocket_codegen)]
#![feature(const_fn)]
#![feature(universal_impl_trait)]
#![feature(generators)]
#![feature(generator_trait)]
#![feature(match_default_bindings)]
#![feature(use_nested_groups)]
#![feature(libc)]
#![allow(unused)]

#[macro_use]
extern crate gfx;
extern crate gfx_text;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate libc;
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

//mod web_api;
//mod control;
//mod serialize;
//mod plugin_loader;
mod gui;

use std::env;
use std::thread;

fn main() {
    /*let inst = env::args()
        .nth(1)
        .map(|name| serialize::from_file(&name))
        .unwrap_or(control::Instance::new());
    let id = env::args()
        .nth(2)
        .map(|id| id.parse().unwrap())
        .unwrap_or(0);*/
    gui::gui_main();
    //thread::spawn(move || {
    //web_api::run_server(inst, id);
    //});
    //thread::park();
}
