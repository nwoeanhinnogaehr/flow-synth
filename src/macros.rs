use modular_flow::graph::*;
use modular_flow::context::*;
use control::*;
use std::thread;
use std::sync::Arc;

pub fn simple_node<F>(
    ctx: Arc<Context>,
    cfg: NewNodeConfig,
    nports: (usize, usize),
    messages: Vec<message::Desc>,
    mut loop_fn: F,
) -> Arc<RemoteControl>
where
    F: FnMut(&NodeContext, &Arc<RemoteControl>) -> Result<()> + Send + 'static,
{
    let node_id = cfg.node.unwrap_or_else(|| ctx.graph().add_node(nports.0, nports.1));
    let node = ctx.graph().node(node_id).unwrap();
    let node_ctx = ctx.node_ctx(node_id).unwrap();
    let remote_ctl = Arc::new(RemoteControl::new(ctx, node, messages));
    remote_ctl.set_saved_data(&cfg.saved_data);

    let ctl = remote_ctl.clone();
    thread::spawn(move || while !ctl.stopped() {
        match loop_fn(&node_ctx, &ctl) {
            Err(e) => {}
            _ => {} // TODO
        }
    });
    remote_ctl
}

#[macro_export]
macro_rules! ignore_nonfatal {
    ($code:expr) => {
        let result: ::modular_flow::graph::Result<()> = do catch {
            $code
            Ok(())
        };
        match result {
            Err(::modular_flow::graph::Error::Aborted) => result?,
            Err(_) => {},
            Ok(_) => {},
        }
    }
}
