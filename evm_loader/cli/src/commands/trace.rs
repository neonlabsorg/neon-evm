use crate::{
    commands::emulate,
    event_listener::tracer::Tracer,
    rpc::Rpc,
    types::{trace::TracedCall, TxParams},
    BlockOverrides, AccountOverrides,
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;
use crate::errors::NeonCliError;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    rpc_client: &dyn Rpc,
    evm_loader: Pubkey,
    tx: TxParams,
    token: Pubkey,
    chain: u64,
    steps: u64,
    accounts: &[Address],
    enable_return_data: bool,
    block_overrides: Option<BlockOverrides>,
    state_overrides: Option<AccountOverrides>,
) -> Result<TracedCall, NeonCliError> {
    let mut tracer = Tracer::new(enable_return_data);

    let emulation_result = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate::execute(rpc_client, evm_loader, tx, token, chain, steps, accounts, block_overrides, state_overrides)
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
