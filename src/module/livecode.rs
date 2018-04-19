use futures::executor;
use futures::future;
use futures::prelude::*;

use future_ext::{Breaker, FutureWrapExt};
use module::{audio_io::Frame, flow, Module};

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

fn start_simple_processor<F: FnMut(Frame) -> Frame + Send + 'static, Ex: executor::Executor>(
    processor: F,
    in_port: Arc<flow::Port<Frame, ()>>,
    out_port: Arc<flow::Port<(), Frame>>,
    breaker: Breaker,
    mut exec: Ex,
) {
    exec.spawn(Box::new(future::loop_fn(
        (processor, in_port, out_port, breaker),
        |(processor, in_port, out_port, breaker)| {
            in_port
                .write1(())
                .wrap((processor, out_port, breaker))
                .map_err(|((processor, out_port, breaker), (in_port, err))| {
                    (
                        processor,
                        in_port,
                        out_port,
                        breaker,
                        format!("in write1 {:?}", err),
                    )
                })
                .and_then(|((processor, out_port, breaker), in_port)| {
                    in_port.read1().wrap((processor, out_port, breaker)).map_err(
                        |((processor, out_port, breaker), (in_port, err))| {
                            (
                                processor,
                                in_port,
                                out_port,
                                breaker,
                                format!("in read1 {:?}", err),
                            )
                        },
                    )
                })
                .and_then(|((processor, out_port, breaker), (in_port, frame))| {
                    out_port
                        .read1()
                        .wrap((processor, in_port, breaker, frame))
                        .map_err(|((processor, in_port, breaker, frame), (out_port, err))| {
                            (
                                processor,
                                in_port,
                                out_port,
                                breaker,
                                format!("out read1 {:?}", err),
                            )
                        })
                })
                .and_then(move |((mut processor, in_port, breaker, frame), (out_port, _))| {
                    out_port
                        .write1(processor(frame))
                        .wrap((processor, in_port, breaker))
                        .map_err(|((processor, in_port, breaker), (out_port, err))| {
                            (
                                processor,
                                in_port,
                                out_port,
                                breaker,
                                format!("out write1 {:?}", err),
                            )
                        })
                })
                .recover(|(processor, in_port, out_port, breaker, err)| {
                    println!("err: {}", err);
                    ((processor, in_port, breaker), out_port)
                })
                .map(|((processor, in_port, breaker), out_port)| {
                    if breaker.test() {
                        future::Loop::Break(())
                    } else {
                        future::Loop::Continue((processor, in_port, out_port, breaker))
                    }
                })
        },
    ))).unwrap();
}

enum Command {
    NewFile(String),
}

pub struct LiveCode {
    ifc: Arc<flow::Interface>,
    in_port: Arc<flow::Port<Frame, ()>>,
    out_port: Arc<flow::Port<(), Frame>>,
    breaker: Breaker,
    cmd_rx: Receiver<Command>,
    cmd_tx: Option<Sender<Command>>,
}

fn process(mut frame: Frame) -> Frame {
    //temporary hack
    frame.data.mapv_inplace(|x| x.abs());
    frame
}
impl Module for LiveCode {
    fn new(ifc: Arc<flow::Interface>) -> LiveCode {
        let in_port = ifc.add_port("Input".into());
        let out_port = ifc.add_port("Output".into());
        let (cmd_tx, cmd_rx) = mpsc::channel();
        LiveCode {
            ifc,
            in_port,
            out_port,
            breaker: Breaker::new(),
            cmd_rx,
            cmd_tx: Some(cmd_tx),
        }
    }
    fn start<Ex: executor::Executor>(&mut self, exec: Ex) {
        start_simple_processor(
            process,
            self.in_port.clone(),
            self.out_port.clone(),
            self.breaker.clone(),
            exec,
        );
    }
    fn name() -> &'static str {
        "Livecode"
    }
    fn stop(&mut self) {
        self.breaker.brake();
    }
    fn ports(&self) -> Vec<Arc<flow::OpaquePort>> {
        self.ifc.ports()
    }
}
use gfx_device_gl as gl;
use gui::{button::*, component::*, event::*, geom::*, module_gui::*, render::*};
struct LiveCodeGui {
    bounds: Box3,
    open_button: Button,
    cmd_tx: Sender<Command>,
}
const PADDING: f32 = 4.0;
impl ModuleGui for LiveCode {
    fn new_body(&mut self, ctx: &mut RenderContext, bounds: Box3) -> Box<GuiComponent<BodyUpdate>> {
        Box::new(LiveCodeGui {
            cmd_tx: self.cmd_tx.take().unwrap(),
            bounds,
            open_button: Button::new(
                ctx.clone(),
                "Pick file".into(),
                Box3 {
                    pos: bounds.pos + Pt3::new(PADDING, PADDING, 0.0),
                    size: Pt3::new(bounds.size.x - PADDING * 2.0, 26.0, 0.0),
                },
            ),
        })
    }
}
impl GuiComponent<bool> for LiveCodeGui {
    fn set_bounds(&mut self, bounds: Box3) {
        self.bounds = bounds;
    }
    fn bounds(&self) -> Box3 {
        self.bounds
    }
    fn render(&mut self, device: &mut gl::Device, ctx: &mut RenderContext) {
        self.open_button.render(device, ctx);
    }
    fn handle(&mut self, event: &Event) -> BodyUpdate {
        match self.open_button.handle(event) {
            ButtonUpdate::Unchanged => false,
            ButtonUpdate::NeedRender => true,
            ButtonUpdate::Clicked => {
                use nfd;
                match nfd::open_file_dialog(None, None).unwrap() {
                    nfd::Response::Okay(path) => {
                        self.open_button.set_label(path.clone());
                        self.cmd_tx.send(Command::NewFile(path)).unwrap();
                    }
                    nfd::Response::Cancel => println!("selection cancelled"),
                    _ => panic!(),
                }
                true
            }
        }
    }
}
