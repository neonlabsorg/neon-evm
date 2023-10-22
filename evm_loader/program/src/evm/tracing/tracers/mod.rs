use crate::evm::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::evm::tracing::tracers::struct_logger::StructLogger;
use crate::evm::tracing::TraceConfig;
use crate::evm::tracing::TracerType;
use ethnum::U256;
use std::cell::RefCell;
use std::rc::Rc;

pub mod openeth;
pub mod struct_logger;

pub fn new_tracer(
    gas_used: Option<U256>,
    trace_config: &TraceConfig,
) -> crate::error::Result<TracerType> {
    Ok(Rc::new(RefCell::new(
        match trace_config.tracer.as_deref() {
            None | Some("") => Box::new(StructLogger::new(gas_used, trace_config)),
            Some("openethereum") => Box::new(OpenEthereumTracer::new(
                serde_json::from_value(trace_config.tracer_config.clone().unwrap()).unwrap(),
            )),
            _ => {
                return Err(crate::error::Error::Custom(format!(
                    "Unsupported tracer: {:?}",
                    trace_config.tracer
                )))
            }
        },
    )))
}
