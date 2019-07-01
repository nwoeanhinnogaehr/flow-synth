use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use futures::executor;
use futures::future;
use futures::prelude::*;

use notify::*;

use future_ext::{Breaker, FutureWrapExt};
use module::{audio_io::Frame, flow, Module};

use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

#[derive(Debug)]
enum UserCommand {
    NewFile(String),
}

pub struct LiveCode {
    ifc: Arc<flow::Interface>,
    in_port: Arc<flow::Port<Frame, ()>>,
    out_port: Arc<flow::Port<(), Frame>>,
    breaker: Breaker,
    cmd_rx: Option<UnboundedReceiver<UserCommand>>,
    cmd_tx: Option<UnboundedSender<UserCommand>>,
    watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
    child: Arc<Mutex<Option<process::Child>>>,
}

impl Drop for LiveCode {
    fn drop(&mut self) {
        self.child.lock().unwrap().take().map(|mut child| {
            println!("killing leftover child");
            child.kill()
        });
    }
}

impl Module for LiveCode {
    fn new(ifc: Arc<flow::Interface>) -> LiveCode {
        let in_port = ifc.get_or_create_port("Input".into());
        let out_port = ifc.get_or_create_port("Output".into());
        let (cmd_tx, cmd_rx) = mpsc::unbounded();
        LiveCode {
            ifc,
            in_port,
            out_port,
            breaker: Breaker::new(),
            cmd_rx: Some(cmd_rx),
            cmd_tx: Some(cmd_tx),
            watcher: Arc::default(),
            child: Arc::default(),
        }
    }

    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        let cmd_rx = self.cmd_rx.take().unwrap();
        let watcher_handle = self.watcher.clone();
        let child_handle = self.child.clone();
        exec.spawn(Box::new(
            cmd_rx
                .for_each(move |event| {
                    println!("event {:?}", event);
                    match event {
                        UserCommand::NewFile(filename) => {
                            let (tx, rx) = ::std::sync::mpsc::channel();
                            let mut watcher: RecommendedWatcher =
                                Watcher::new(tx, Duration::from_secs(1)).unwrap();
                            // watch parent dir and filter later, because if we just watch the file and
                            // it gets removed it will stop watching it
                            let parent_dir = Path::new(&filename).parent().unwrap();
                            watcher.watch(parent_dir, RecursiveMode::NonRecursive).unwrap();
                            // store it globally because otherwise it gets dropped and stops watching
                            *watcher_handle.lock().unwrap() = Some(watcher);
                            let child_handle = child_handle.clone();
                            spawn_child(&child_handle, PathBuf::from(&filename));

                            // TODO
                            // This thread gets leaked, as does the thread spawned internally inside
                            // the watcher... the notify crate is not cleaning up properly.
                            // Not sure if it's mio that's broken or what.
                            thread::spawn(move || loop {
                                match rx.recv() {
                                    Ok(event) => {
                                        println!("{:?}", event);
                                        match event {
                                            DebouncedEvent::Write(path) => {
                                                if path.to_str().unwrap() != filename {
                                                    continue;
                                                }
                                                spawn_child(&child_handle, path);
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(e) => {
                                        println!("Watcher thread done: {:?}", e);
                                        return;
                                    }
                                }
                            });
                        }
                    }
                    Ok(())
                })
                .then(|x| Ok(())),
        )).unwrap();

        let child_handle = self.child.clone();
        start_simple_processor(
            move |mut frame: Frame| -> Frame {
                let mut guard = child_handle.lock().unwrap();
                let child = match *guard {
                    Some(ref mut child) => child,
                    None => return frame,
                };
                let stdin = child.stdin.as_mut().unwrap();
                let stdout = child.stdout.as_mut().unwrap();
                //temporary hack
                //need to use futures for io
                use std::io::{Read, Write};
                use std::mem;
                let mut buffer: Vec<_> = frame.data.iter().cloned().collect();
                let bytes: &mut [u8] = unsafe {
                    ::std::slice::from_raw_parts_mut(
                        buffer.as_ptr() as *mut u8,
                        buffer.len() * mem::size_of::<f32>(),
                    )
                };
                stdin.write(bytes).unwrap();
                stdout.read(bytes).unwrap();
                for (outs, ins) in frame.data.iter_mut().zip(buffer.drain(..)) {
                    *outs = ins;
                }
                frame
            },
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
fn spawn_child(child_handle: &Arc<Mutex<Option<process::Child>>>, path: PathBuf) {
    let child = match process::Command::new(path)
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            println!("err spawning: {:?}", e);
            return;
        }
    };

    let mut child_handle = child_handle.lock().unwrap();
    child_handle.take().map(|mut child| {
        println!("killing previous child");
        child.kill().unwrap();
    });
    *child_handle = Some(child);
}

use gfx_device_gl as gl;
use gui::{button::*, component::*, event::*, geom::*, module_gui::*, render::*};
struct LiveCodeGui {
    bounds: Box3,
    open_button: Button,
    cmd_tx: UnboundedSender<UserCommand>,
}
const PADDING: f32 = 4.0;
impl ModuleGui for LiveCode {
    fn new_body(&mut self, ctx: &mut RenderContext, bounds: Box3) -> Box<dyn GuiComponent<BodyUpdate>> {
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
                match nfd::open_file_dialog(None, None).unwrap() {
                    nfd::Response::Okay(path) => {
                        self.open_button.set_label(path.clone());
                        self.cmd_tx.unbounded_send(UserCommand::NewFile(path)).unwrap();
                    }
                    nfd::Response::Cancel => println!("selection cancelled"),
                    _ => panic!(),
                }
                true
            }
        }
    }
}
