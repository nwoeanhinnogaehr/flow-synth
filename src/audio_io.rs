use modular_flow::graph::*;
use modular_flow::context::*;
use jack::prelude::*;
use std::thread;

pub struct AudioIONode {
    pub id: NodeID,
    pub inputs: usize,
    pub outputs: usize,
}

impl AudioIONode {
    pub fn new(graph: &mut Graph, inputs: usize, outputs: usize) -> AudioIONode {
        AudioIONode {
            id: graph.add_node(inputs, outputs),
            inputs,
            outputs
        }
    }
    pub fn run(self, ctx: &Context) {
        let node_ctx = ctx.node_ctx(self.id).unwrap();

        thread::spawn(move || {
            // create client
            let (client, _status) =
                Client::new("flow-synth", client_options::NO_START_SERVER).unwrap();

            // create ports
            let inputs: Vec<_> = (0..self.inputs).map(|i| client
                .register_port(&format!("in-{}", i), AudioInSpec::default())
                .unwrap()).collect();
            let mut outputs: Vec<_> = (0..self.outputs).map(|i| client
                .register_port(&format!("out-{}", i), AudioOutSpec::default())
                .unwrap()).collect();

            let unowned_inputs: Vec<_> = inputs.iter().map(|x| x.clone_unowned()).collect();
            let unowned_outputs: Vec<_> = outputs.iter().map(|x| x.clone_unowned()).collect();

            let processor =
                ClosureProcessHandler::new(move |_: &Client, ps: &ProcessScope| -> JackControl {
                    // get port buffers
                    let input_ports: Vec<_> = inputs.iter().map(|input| AudioInPort::new(input, ps)).collect();
                    let mut output_ports: Vec<_> = outputs.iter_mut().map(|output| AudioOutPort::new(output, ps)).collect();

                    // shuffle data
                    let mut lock = node_ctx.lock();
                    for (idx, input) in input_ports.iter().enumerate() {
                        lock.write(OutPortID(idx), input).unwrap();
                    }
                    for (idx, output) in output_ports.iter_mut().enumerate() {
                        // to avoid xruns, don't block, just skip instead.
                        if lock.available::<f32>(InPortID(idx)) >= output.len() {
                            let read = lock.read_n(InPortID(idx), output.len()).unwrap();
                            output.copy_from_slice(&read);
                        }
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
            let physical_playback = active_client.ports(None, None, port_flags::IS_INPUT | port_flags::IS_PHYSICAL);
            for (physical, ours) in physical_playback.iter().map(|name| active_client.port_by_name(name).unwrap()).zip(unowned_outputs.iter()) {
                active_client.connect_ports(ours, &physical).unwrap();
            }
            // connect our inputs to the pulse sink
            let pulse_sink = active_client.ports(Some("PulseAudio"), None, port_flags::IS_OUTPUT);
            for (pulse, ours) in pulse_sink.iter().map(|name| active_client.port_by_name(name).unwrap()).zip(unowned_inputs.iter()) {
                active_client.connect_ports(&pulse, ours).unwrap();
            }
            thread::park();
        });
    }
}
