use crate::tracing::tracers::openeth::tracer::OpenEthereumTracer;
use crate::tracing::tracers::struct_logger::StructLogger;
use crate::tracing::TraceConfig;
use ethnum::U256;
use evm_loader::evm::tracing::TracerType;

pub mod openeth;
pub mod struct_logger;

pub fn new_tracer(
    gas_used: Option<U256>,
    trace_config: &TraceConfig,
) -> evm_loader::error::Result<TracerType> {
    Ok(match trace_config.tracer.as_deref() {
        None | Some("") => Box::new(StructLogger::new(gas_used, trace_config)),
        Some("openethereum") => Box::new(OpenEthereumTracer::new(
            serde_json::from_value(trace_config.tracer_config.clone().unwrap()).unwrap(),
        )),
        _ => {
            return Err(evm_loader::error::Error::Custom(format!(
                "Unsupported tracer: {:?}",
                trace_config.tracer
            )))
        }
    })
}
