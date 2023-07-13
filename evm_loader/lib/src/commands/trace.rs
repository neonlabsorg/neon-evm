use crate::{commands::emulate, context::Context, types::TxParams, Config, NeonResult};
use evm_loader::evm::event_listener::trace::TracedCall;
use evm_loader::evm::event_listener::tracer::Tracer;
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;

pub type TraceReturn = TracedCall;

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    config: &Config,
    context: &Context,
    tx: TxParams,
    token: Pubkey,
    chain: u64,
    steps: u64,
    accounts: &[Address],
    solana_accounts: &[Pubkey],
) -> NeonResult<TraceReturn> {
    let tracer = Tracer::new();

    emulate::execute(
        config,
        context,
        tx,
        token,
        chain,
        steps,
        accounts,
        solana_accounts,
    )
    .await?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    let trace = TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: 0,
    };

    Ok(trace)
}
