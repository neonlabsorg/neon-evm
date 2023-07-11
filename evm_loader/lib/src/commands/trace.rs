use crate::{
    commands::emulate,
    context::Context,
    event_listener::tracer::Tracer,
    types::{trace::TracedCall, TxParams},
    Config, NeonResult,
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;
use std::fmt;

pub type TraceReturn = TracedCall;

impl fmt::Display for TraceReturn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "(full_trace_data size: {}, used_gas: {}, ...)",
            self.full_trace_data.len(),
            self.used_gas
        )
    }
}

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
    let mut tracer = Tracer::new();

    evm_loader::evm::tracing::using(&mut tracer, || async {
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
        .await
    })
    .await?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    let trace = TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: 0,
    };

    Ok(trace)
}
