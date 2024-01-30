use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
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

#[derive(Debug)]
#[enum_delegate::implement(EventListener)] // cannot use enum_dispatch because of trait and enum in different crates
pub enum TracerTypeEnum {
    StructLogger(StructLogger),
    OpenEthereumTracer(OpenEthereumTracer),
    PrestateTracer(PrestateTracer),
}

pub fn new_tracer(
    gas_used: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerTypeEnum> {
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(TracerTypeEnum::StructLogger(StructLogger::new(
            gas_used,
            trace_config,
        ))),
        Some("openethereum") => Ok(TracerTypeEnum::OpenEthereumTracer(OpenEthereumTracer::new(
            trace_config,
        ))),
        Some("prestateTracer") => Ok(TracerTypeEnum::PrestateTracer(PrestateTracer::new(
            trace_config,
        ))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}
