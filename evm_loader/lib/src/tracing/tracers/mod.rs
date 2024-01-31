use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use evm_loader::evm::tracing::{Event, EventListener};
use serde_json::Value;

pub mod struct_logger;

#[derive(Debug)]
pub enum TracerTypeEnum {
    StructLogger(StructLogger),
}

// enum_dispatch requires both trait and enum to be defined in the same crate
impl EventListener for TracerTypeEnum {
    fn event(&mut self, event: Event) {
        match self {
            TracerTypeEnum::StructLogger(struct_logger) => struct_logger.event(event),
        }
    }

    fn into_traces(self) -> Value {
        match self {
            TracerTypeEnum::StructLogger(struct_logger) => struct_logger.into_traces(),
        }
    }
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
