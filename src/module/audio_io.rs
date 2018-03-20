use futures::prelude::*;
use futures::executor;
use futures::future;
use futures::channel::mpsc;
use futures::task;

use module::{flow, Module};
use future_ext::{Breaker, FutureWrapExt};

use jack::*;

use ndarray::{Array, Array2, Axis};

use std::sync::Arc;

struct Frame {
    pub rate: f32,
    pub data: Array2<f32>,
}
pub struct AudioIO {
    ifc: Arc<flow::Interface>,
    in_port: Option<Arc<flow::Port<Frame, ()>>>,
    out_port: Option<Arc<flow::Port<(), Frame>>>,
    breaker: Breaker,
}
impl Module for AudioIO {
    fn new(ifc: Arc<flow::Interface>) -> AudioIO {
        let in_port = Some(ifc.add_port("Input".into()));
        let out_port = Some(ifc.add_port("Output".into()));
        AudioIO {
            ifc,
            in_port,
            out_port,
            breaker: Breaker::new(),
        }
    }
    fn name() -> &'static str {
        "AudioIO"
    }
    fn start<Ex: executor::Executor>(&mut self, mut exec: Ex) {
        exec.spawn(Box::new(AudioIOFuture::new(self))).unwrap();
    }
    fn stop(&mut self) {
        self.breaker.brake();
    }
    fn ports(&self) -> Vec<Arc<flow::OpaquePort>> {
        self.ifc.ports()
    }
}

struct AudioIOFuture {
    client: Option<AsyncClient<(), Processor>>,
    future: Box<Future<Item = (), Error = Never> + Send>,
    output_rx: Option<mpsc::Receiver<Frame>>,
    input_tx: Option<mpsc::Sender<Frame>>,
    breaker: Breaker,
}

impl AudioIOFuture {
    fn new(base: &mut AudioIO) -> AudioIOFuture {
        let (input_tx, input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);
        let in_port = base.in_port.take().unwrap();
        let out_port = base.out_port.take().unwrap();
        let in_future = future::loop_fn(
            (input_rx, out_port, base.breaker.clone()),
            |(recv, port, breaker)| {
                port.read1()
                    .wrap(recv)
                    .map_err(|(recv, (port, err))| (recv, port, format!("read1 {:?}", err)))
                    .and_then(|(recv, (port, _req))| {
                        recv.into_future()
                            .map(|(frame, recv)| (frame.unwrap(), recv, port))
                            .map_err(|(_err, _recv)| panic!()) // error: Never, panic impossible!
                    })
                    .and_then(|(frame, recv, port)| {
                        port.write1(frame)
                            .wrap(recv)
                            .map_err(|(recv, (port, err))| (recv, port, format!("write1 {:?}", err)))
                    })
                    .recover(|(recv, port, err)| {
                        println!("In err: {}", err);
                        (recv, port)
                    })
                    .map(|(recv, port)| {
                        if breaker.test() {
                            future::Loop::Break(())
                        } else {
                            future::Loop::Continue((recv, port, breaker))
                        }
                    })
            },
        );
        let out_future = future::loop_fn(
            (output_tx, in_port, base.breaker.clone()),
            |(tx, port, breaker)| {
                port.write1(())
                    .wrap(tx)
                    .map_err(|(tx, (port, err))| (tx, port, format!("write1 {:?}", err)))
                    .and_then(|(tx, port)| {
                        port.read1()
                            .wrap(tx)
                            .map_err(|(tx, (port, err))| (tx, port, format!("read1 {:?}", err)))
                    })
                    .and_then(|(tx, (port, frame))| {
                        tx.send(frame).map(|tx| (tx, port)).map_err(|err| panic!())
                        // error: Never, panic impossible!
                    })
                    .recover(|(tx, port, err)| {
                        println!("Out err: {}", err);
                        (tx, port)
                    })
                    .map(|(tx, port)| {
                        if breaker.test() {
                            future::Loop::Break(())
                        } else {
                            future::Loop::Continue((tx, port, breaker))
                        }
                    })
            },
        );
        AudioIOFuture {
            client: None,
            input_tx: Some(input_tx),
            output_rx: Some(output_rx),
            future: Box::new(in_future.join(out_future).map(|((), ())| ())),
            breaker: base.breaker.clone(),
        }
    }
    fn initialize(&mut self) {
        // setup jack
        if self.client.is_none() {
            let n_inputs = 2;
            let n_outputs = 2;
            let (client, _status) = Client::new("flow-synth", ClientOptions::NO_START_SERVER).unwrap();
            // create ports
            let inputs: Vec<_> = (0..n_inputs)
                .map(|i| {
                    client
                        .register_port(&format!("in-{}", i), AudioIn::default())
                        .unwrap()
                })
                .collect();
            let outputs: Vec<_> = (0..n_outputs)
                .map(|i| {
                    client
                        .register_port(&format!("out-{}", i), AudioOut::default())
                        .unwrap()
                })
                .collect();

            // activate the client
            let processor = Processor {
                inputs,
                outputs,
                input_tx: self.input_tx.take().unwrap(),
                output_rx: self.output_rx.take().unwrap(),
                breaker: self.breaker.clone(),
            };
            self.client = Some(AsyncClient::new(client, (), processor).unwrap());
        }
    }
}
impl Future for AudioIOFuture {
    type Item = ();
    type Error = Never;
    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        self.initialize();
        self.future.poll(cx)
    }
}

struct Processor {
    inputs: Vec<Port<AudioIn>>,
    outputs: Vec<Port<AudioOut>>,
    input_tx: mpsc::Sender<Frame>,
    output_rx: mpsc::Receiver<Frame>,
    breaker: Breaker,
}
impl ProcessHandler for Processor {
    fn process(&mut self, client: &Client, ps: &ProcessScope) -> Control {
        let in_frame = Frame {
            rate: client.sample_rate() as f32,
            data: Array::from_iter(
                self.inputs
                    .iter()
                    .flat_map(|input| input.as_slice(ps).to_vec()),
            ).into_shape((self.inputs.len(), client.buffer_size() as usize))
                .unwrap()
                .reversed_axes(),
        };

        // ignore errors, prefer to drop the frame
        if let Ok(frame) = self.output_rx.try_next() {
            let frame = frame.unwrap();
            assert!(frame.rate == in_frame.rate);
            assert!(frame.data.shape() == in_frame.data.shape());
            for (output, buffer) in self.outputs.iter_mut().zip(frame.data.axis_iter(Axis(1))) {
                for (sample_out, sample) in output.as_mut_slice(ps).iter_mut().zip(buffer.iter()) {
                    *sample_out = *sample;
                }
            }
        } else {
            for output in &mut self.outputs {
                for sample in output.as_mut_slice(ps) {
                    *sample = 0.0;
                }
            }
        }
        let _ = self.input_tx.try_send(in_frame);

        if self.breaker.test() {
            Control::Quit
        } else {
            Control::Continue
        }
    }
}
