use evm_loader::evm::tracing::EventListener;
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;

use crate::commands::get_config::BuildConfigSimulator;
use crate::errors::NeonError;
use crate::rpc::Rpc;
use crate::tracing::tracers::new_tracer;
use crate::types::EmulateRequest;

pub async fn trace_transaction(
    rpc: &(impl Rpc + BuildConfigSimulator),
    program_id: Pubkey,
    emulate_request: EmulateRequest,
) -> Result<Value, NeonError> {
    let trace_config = emulate_request
        .trace_config
        .as_ref()
        .map(|c| c.trace_config.clone())
        .unwrap_or_default();

    let tracer = new_tracer(&trace_config)?;

    let (r, tracer) =
        super::emulate::execute(rpc, program_id, emulate_request, Some(tracer)).await?;

    let mut traces = tracer.expect("tracer should not be None").into_traces();
    traces["gas"] = r.used_gas.into();

    Ok(traces)
}
