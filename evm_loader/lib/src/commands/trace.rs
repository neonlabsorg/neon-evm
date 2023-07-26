use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{
    account_storage::EmulatorAccountStorage,
    commands::emulate::emulate_trx,
    errors::NeonError,
    event_listener::tracer::Tracer,
    types::{
        trace::{TraceConfig, TracedCall},
        TxParams,
    },
};

#[derive(Serialize, Deserialize)]
pub struct TraceBlockReturn(pub Vec<TracedCall>);

impl Display for TraceBlockReturn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ traced call(s): {} }}", self.0.len())
    }
}

pub async fn trace_block(
    transactions: Vec<TxParams>,
    chain_id: u64,
    steps: u64,
    trace_config: &TraceConfig,
    storage: EmulatorAccountStorage<'_>,
) -> Result<TraceBlockReturn, NeonError> {
    let mut results = vec![];

    for tx_params in transactions {
        let result = trace_trx(tx_params, &storage, chain_id, steps, trace_config)?;
        results.push(result);
    }

    Ok(TraceBlockReturn(results))
}

pub fn trace_trx<'a>(
    tx_params: TxParams,
    storage: &'a EmulatorAccountStorage<'a>,
    chain_id: u64,
    steps: u64,
    trace_config: &TraceConfig,
) -> Result<TracedCall, NeonError> {
    let mut tracer = Tracer::new(trace_config.enable_return_data);

    let emulation_result = evm_loader::evm::tracing::using(&mut tracer, || {
        emulate_trx(tx_params, storage, chain_id, steps)
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
