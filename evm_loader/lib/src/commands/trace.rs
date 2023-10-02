use serde_json::Value;
use std::rc::Rc;

use crate::account_storage::EmulatorAccountStorage;
use crate::commands::emulate::emulate_trx;
use crate::types::request_models::TxParamsRequestModel;
use crate::types::EmulationParams;
use crate::{errors::NeonError, RequestContext};
use evm_loader::evm::tracing::tracers::new_tracer;
use evm_loader::evm::tracing::TraceCallConfig;

#[allow(clippy::too_many_arguments)]
pub async fn trace_transaction(
    context: &RequestContext<'_>,
    tx_params: TxParamsRequestModel,
    emulation_params: &EmulationParams,
    trace_call_config: &TraceCallConfig,
) -> Result<Value, NeonError> {
    let tracer = new_tracer(&trace_call_config.trace_config)?;

    let storage = EmulatorAccountStorage::with_accounts(
        context,
        emulation_params,
        trace_call_config.block_overrides.as_ref(),
        trace_call_config.state_overrides.as_ref(),
    )
    .await?;

    let emulation_result = emulate_trx(
        tx_params,
        emulation_params,
        &storage,
        Some(Rc::clone(&tracer)),
    )
    .await?;

    Ok(Rc::try_unwrap(tracer)
        .expect("There is must be only one reference")
        .into_inner()
        .into_traces(emulation_result))
}
