use crate::{
    commands::emulate,
    event_listener::tracer::Tracer,
    errors::NeonCliError,
    rpc::Rpc,
    types::{
        trace::{TracedCall, TraceCallConfig},
        TxParams,
    },
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    tx: TxParams,
    token: Pubkey,
    chain: u64,
    steps: u64,
    accounts: &[Address],
    trace_call_config: TraceCallConfig,
) -> Result<TracedCall, NeonCliError> {
    let mut tracer = Tracer::new(trace_call_config.trace_config.enable_return_data);

    let emulation_result = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate::execute(rpc_client, evm_loader, tx, token, chain, steps, accounts, trace_call_config)
    })?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    Ok(TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result.used_gas,
        result: emulation_result.result,
        exit_status: emulation_result.exit_status,
    })
}
