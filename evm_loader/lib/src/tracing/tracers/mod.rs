use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Event, EventListener};
use serde_json::Value;

pub mod struct_logger;

#[derive(Debug)]
#[enum_delegate::implement(EventListener)] // cannot use enum_dispatch because of trait and enum in different crates
pub enum TracerTypeEnum {
    StructLogger(StructLogger),
}

pub fn new_tracer(trace_config: &TraceConfig) -> evm_loader::error::Result<TracerTypeEnum> {
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(TracerTypeEnum::StructLogger(StructLogger::new(
            trace_config,
        ))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}
