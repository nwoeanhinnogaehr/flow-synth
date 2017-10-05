use modular_flow::graph::Result;
use modular_flow::context::*;
use jack::prelude::*;
use std::thread;
use super::control::{NodeDescriptor, RemoteControl};
use std::sync::Arc;

pub struct AudioIO {}

impl NodeDescriptor for AudioIO {
    const NAME: &'static str = "audio I/O";
    fn new(ctx: Arc<Context>) -> Arc<RemoteControl> {
        let id = ctx.graph().add_node(2, 2);
        let node_ctx = ctx.node_ctx(id).unwrap();
        let node = ctx.graph().node(id).unwrap();
        let remote_ctl = Arc::new(RemoteControl::new(ctx, node, Vec::new()));
        let ctl = remote_ctl.clone();
        let n_inputs = node_ctx.node().in_ports().len();
        let n_outputs = node_ctx.node().out_ports().len();
        thread::spawn(move || {
            // create client
            let (client, _status) = Client::new("flow-synth", client_options::NO_START_SERVER).unwrap();

            // create ports
            let inputs: Vec<_> = (0..n_inputs)
                .map(|i| client.register_port(&format!("in-{}", i), AudioInSpec::default()).unwrap())
                .collect();
            let mut outputs: Vec<_> = (0..n_outputs)
                .map(|i| client.register_port(&format!("out-{}", i), AudioOutSpec::default()).unwrap())
                .collect();

            let unowned_inputs: Vec<_> = inputs.iter().map(|x| x.clone_unowned()).collect();
            let unowned_outputs: Vec<_> = outputs.iter().map(|x| x.clone_unowned()).collect();
            let inner_ctl = ctl.clone();

            let processor =
                ClosureProcessHandler::new(move |client: &Client, ps: &ProcessScope| -> JackControl {
                    if inner_ctl.stopped() {
                        return JackControl::Quit;
                    }

                    let res: Result<()> = do catch {
                        // get port buffers
                        let input_ports: Vec<_> =
                            inputs.iter().map(|input| AudioInPort::new(input, ps)).collect();
                        let mut output_ports: Vec<_> =
                            outputs.iter_mut().map(|output| AudioOutPort::new(output, ps)).collect();

                        // shuffle data
                        let lock = node_ctx.lock(&node_ctx.node().in_ports(), &node_ctx.node().out_ports());
                        for (input, out_port) in input_ports.into_iter().zip(lock.node().out_ports()) {
                            // discard errors, drop the frame
                            let _ = lock.write(out_port.id(), &input);
                        }
                        // to avoid xruns, don't block, just skip instead.
                        if lock.node().in_ports().iter().all(|in_port| {
                            lock.available::<f32>(in_port.id()).unwrap_or(0) >= client.buffer_size() as usize
                        }) {
                            for (output, in_port) in output_ports.iter_mut().zip(lock.node().in_ports()) {
                                // discard errors, drop the frame
                                let _ = lock.read_n(in_port.id(), client.buffer_size() as usize)
                                    .map(|read| output.copy_from_slice(&read));
                            }
                        }
                        Ok(())
                    };
                    if let Err(e) = res {
                        println!("audio {:?}", e);
                    }

                    // do it forever
                    JackControl::Continue
                });

            // activate the client
            let active_client = AsyncClient::new(client, (), processor).unwrap();

            // connect ports
            // TODO not sure what to do with this long term,
            // might just leave it disconnected, or connect to sys in/out
            // connect our outputs to physical playback
            let physical_playback =
                active_client.ports(None, None, port_flags::IS_INPUT | port_flags::IS_PHYSICAL);
            for (physical, ours) in physical_playback
                .iter()
                .map(|name| active_client.port_by_name(name).unwrap())
                .zip(unowned_outputs.iter())
            {
                active_client.connect_ports(ours, &physical).unwrap();
            }
            // connect our inputs to the pulse sink
            let pulse_sink = active_client.ports(Some("PulseAudio"), None, port_flags::IS_OUTPUT);
            for (pulse, ours) in pulse_sink
                .iter()
                .map(|name| active_client.port_by_name(name).unwrap())
                .zip(unowned_inputs.iter())
            {
                active_client.connect_ports(&pulse, ours).unwrap();
            }
            ctl.block_until_stopped();
        });
        remote_ctl
    }
}
