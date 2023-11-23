use crate::tracing::TraceConfig;
use evm_loader::evm::tracing::{EmulationResult, Event, EventListener};
use serde::Deserialize;
use serde_json::Value;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L57>
#[derive(Debug)]
pub struct PrestateTracer {
    config: PrestateTracerConfig,
}

impl PrestateTracer {
    pub fn new(trace_config: TraceConfig) -> Self {
        PrestateTracer {
            config: trace_config.into(),
        }
    }
}

impl From<TraceConfig> for PrestateTracerConfig {
    fn from(trace_config: TraceConfig) -> Self {
        let tracer_config = trace_config
            .tracer_config
            .expect("tracer_config should not be None for \"prestateTracer\" tracer");
        serde_json::from_value(tracer_config).expect("tracer_config should be PrestateTracerConfig")
    }
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L72>
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PrestateTracerConfig {
    diff_mode: bool,
}

impl EventListener for PrestateTracer {
    fn event(&mut self, _event: Event) {}

    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value {
        if self.config.diff_mode {
            serde_json::to_value(emulation_result.states)
        } else {
            serde_json::to_value(emulation_result.states.pre)
        }
        .expect("Conversion error")
    }
}
