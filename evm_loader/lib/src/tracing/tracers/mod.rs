use crate::tracing::tracers::call_tracer::CallTracer;
use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use crate::types::TxParams;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::Event;
use evm_loader::evm::tracing::EventListener;
use serde_json::Value;

pub mod call_tracer;
pub mod openeth;
pub mod prestate_tracer;
pub mod state_diff;
pub mod struct_logger;

#[enum_dispatch(Tracer)]
pub enum TracerTypeEnum {
    StructLogger(StructLogger),
    OpenEthereumTracer(OpenEthereumTracer),
    PrestateTracer(PrestateTracer),
    CallTracer(CallTracer),
}

// cannot use enum_dispatch because of trait and enum in different crates
#[async_trait(?Send)]
impl EventListener for TracerTypeEnum {
    async fn event(
        &mut self,
        executor_state: &impl Database,
        event: Event,
    ) -> evm_loader::error::Result<()> {
        match self {
            Self::StructLogger(tracer) => tracer.event(executor_state, event).await,
            Self::OpenEthereumTracer(tracer) => tracer.event(executor_state, event).await,
            Self::PrestateTracer(tracer) => tracer.event(executor_state, event).await,
            Self::CallTracer(tracer) => tracer.event(executor_state, event).await,
        }
    }
}

#[enum_dispatch]
pub trait Tracer: EventListener {
    fn into_traces(self, emulator_gas_used: u64) -> Value;
}

pub fn new_tracer(
    tx: &TxParams,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerTypeEnum> {
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(TracerTypeEnum::StructLogger(StructLogger::new(
            trace_config,
            tx,
        ))),
        Some("openethereum") => Ok(TracerTypeEnum::OpenEthereumTracer(OpenEthereumTracer::new(
            trace_config,
            tx,
        ))),
        Some("prestateTracer") => Ok(TracerTypeEnum::PrestateTracer(PrestateTracer::new(
            trace_config,
            tx,
        ))),
        Some("callTracer") => Ok(TracerTypeEnum::CallTracer(CallTracer::new(
            trace_config,
            tx,
        ))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}
