use crate::{
    commands::emulate,
    event_listener::tracer::Tracer,
    types::{trace::TracedCall, TxParams},
    Config, NeonCliResult, BlockOverrides, AccountOverrides,
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    config: &Config,
    tx: TxParams,
    token: Pubkey,
    chain: u64,
    steps: u64,
    accounts: &[Address],
    enable_return_data: bool,
    block_overrides: Option<BlockOverrides>,
    state_override: Option<AccountOverrides>,
) -> NeonCliResult {
    let mut tracer = Tracer::new(enable_return_data);

    let emulation_result = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate::execute(config, tx, token, chain, steps, accounts, block_overrides, state_override)
    })?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    let trace = TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result["used_gas"]
            .as_u64()
            .expect("Failed to treat `used_gas` as u64"),
        result: emulation_result["result"]
            .as_str()
            .expect("Failed to treat `result` as string")
            .to_owned(),
        exit_status: emulation_result["exit_status"]
            .as_str()
            .expect("Failed to treat `exit_status` as string")
            .to_owned(),
    };

    Ok(serde_json::json!(trace))
}
