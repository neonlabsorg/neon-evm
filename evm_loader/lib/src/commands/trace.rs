#![allow(clippy::missing_errors_doc)]

use log::info;
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

    let tracer = new_tracer(&emulate_request.tx, trace_config)?;

    let (emulate_response, emulated_traces) =
        super::emulate::execute(rpc, program_id, emulate_request, Some(tracer)).await?;

    info!("trace_emulate_response: {:?}", emulate_response);

    Ok(emulated_traces.expect("traces should not be None"))
}
