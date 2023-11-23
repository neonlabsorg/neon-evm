use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::prestate_tracer::tracer::PrestateTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use ethnum::U256;
use evm_loader::evm::tracing::{EventListener, TracerType};
use std::cell::RefCell;
use std::rc::Rc;

pub mod openeth;
pub mod prestate_tracer;
pub mod state_diff;
pub mod struct_logger;

pub fn new_tracer(
    gas_used: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<TracerType> {
    Ok(Rc::new(RefCell::new(build_tracer(gas_used, trace_config)?)))
}

fn build_tracer(
    gas_used: Option<U256>,
    trace_config: TraceConfig,
) -> evm_loader::error::Result<Box<dyn EventListener>> {
    match trace_config.tracer.as_deref() {
        None | Some("") => Ok(Box::new(StructLogger::new(gas_used, trace_config))),
        Some("openethereum") => Ok(Box::new(OpenEthereumTracer::new(trace_config))),
        Some("prestateTracer") => Ok(Box::new(PrestateTracer::new(trace_config))),
        _ => Err(evm_loader::error::Error::Custom(format!(
            "Unsupported tracer: {:?}",
            trace_config.tracer
        ))),
    }
}
