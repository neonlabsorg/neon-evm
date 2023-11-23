use std::rc::Rc;

use evm_loader::evm::tracing::{EmulationResult, TracerType};
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;

use crate::commands::emulate::EmulateResponse;
use crate::tracing::tracers::new_tracer;
use crate::types::EmulateRequest;
use crate::{errors::NeonError, rpc::Rpc};

pub async fn trace_transaction(
    rpc_client: &dyn Rpc,
    program_id: Pubkey,
    config: EmulateRequest,
) -> Result<Value, NeonError> {
    let trace_config = config
        .trace_config
        .as_ref()
        .map(|c| c.trace_config.clone())
        .unwrap_or_default();

    let tracer = new_tracer(config.tx.gas_used, trace_config)?;

    let emulation_tracer = Some(Rc::clone(&tracer));
    let r = super::emulate::execute(rpc_client, program_id, config, emulation_tracer).await?;

    Ok(into_traces(tracer, to_emulation_result(r)))
}

pub fn to_emulation_result(r: EmulateResponse) -> EmulationResult {
    EmulationResult {
        exit_status: r.exit_status,
        result: r.result,
        steps_executed: r.steps_executed,
        used_gas: r.used_gas,
        states: r.states,
    }
}

pub fn into_traces(tracer: TracerType, emulation_result: EmulationResult) -> Value {
    Rc::try_unwrap(tracer)
        .expect("There is must be only one reference")
        .into_inner()
        .into_traces(emulation_result)
}
