use crate::{
    commands::emulate,
    event_listener::tracer::Tracer,
    types::{trace::TracedCall, TxParams},
    Config, NeonCliResult,
};
use evm_loader::types::Address;
use solana_sdk::pubkey::Pubkey;

pub fn execute(
    config: &Config,
    tx: TxParams,
    token: Pubkey,
    chain: u64,
    steps: u64,
    accounts: &[Address],
) -> NeonCliResult {
    let mut tracer = Tracer::new();

    let emulation_result = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate::execute(config, tx, token, chain, steps, accounts)
    })?;

    let (vm_trace, full_trace_data) = tracer.into_traces();

    let trace = TracedCall {
        vm_trace,
        full_trace_data,
        used_gas: emulation_result["used_gas"].as_u64()
            .expect("Failed to treat `used_gas` as u64"),
    };

    Ok(serde_json::json!(trace))
}
