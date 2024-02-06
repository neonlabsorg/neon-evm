use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use async_trait::async_trait;
use ethnum::U256;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::EmulationResult;
use evm_loader::evm::tracing::Event;
use evm_loader::evm::tracing::EventListener;
use serde_json::Value;

pub mod openeth;
pub mod prestate_tracer;
pub mod state_diff;
pub mod struct_logger;

#[derive(Debug)] // cannot use enum_dispatch because of trait and enum in different crates
pub enum TracerTypeEnum {
    StructLogger(StructLogger),
    OpenEthereumTracer(OpenEthereumTracer),
    PrestateTracer(PrestateTracer),
}

#[async_trait(?Send)]
impl EventListener for TracerTypeEnum {
    async fn event(
        &mut self,
        executor_state: &mut impl Database,
        event: Event,
        chain_id: u64,
    ) -> evm_loader::error::Result<()> {
        match self {
            TracerTypeEnum::StructLogger(tracer) => {
                tracer.event(executor_state, event, chain_id).await
            }
            TracerTypeEnum::OpenEthereumTracer(tracer) => {
                tracer.event(executor_state, event, chain_id).await
            }
            TracerTypeEnum::PrestateTracer(tracer) => {
                tracer.event(executor_state, event, chain_id).await
            }
        }
    }
    fn into_traces(self, emulation_result: EmulationResult) -> Value {
        match self {
            TracerTypeEnum::StructLogger(tracer) => tracer.into_traces(emulation_result),
            TracerTypeEnum::OpenEthereumTracer(tracer) => tracer.into_traces(emulation_result),
            TracerTypeEnum::PrestateTracer(tracer) => tracer.into_traces(emulation_result),
        }
    }
}

pub fn new_tracer(
    gas_used: Option<U256>,
    gas_price: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerTypeEnum> {
    let tx_fee = gas_used
        .unwrap_or_default()
        .saturating_mul(gas_price.unwrap_or_default());
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(TracerTypeEnum::StructLogger(StructLogger::new(
            gas_used,
            trace_config,
        ))),
        Some("openethereum") => Ok(TracerTypeEnum::OpenEthereumTracer(OpenEthereumTracer::new(
            tx_fee,
            trace_config,
        ))),
        Some("prestateTracer") => Ok(TracerTypeEnum::PrestateTracer(PrestateTracer::new(
            tx_fee,
            trace_config,
        ))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}
