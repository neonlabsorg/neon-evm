use crate::tracing::tracers::prestate_tracer::state_diff::{
    build_prestate_tracer_diff_mode_result, build_prestate_tracer_pre_state,
};
use crate::tracing::TraceConfig;
use evm_loader::evm::tracing::{EmulationResult, Event, EventListener};
use serde::{Deserialize, Serialize};
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
        trace_config
            .tracer_config
            .map(|tracer_config| {
                serde_json::from_value(tracer_config)
                    .expect("tracer_config should be PrestateTracerConfig")
            })
            .unwrap_or_default()
    }
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L72>
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrestateTracerConfig {
    pub diff_mode: bool,
}

impl EventListener for PrestateTracer {
    fn event(&mut self, _event: Event) {}

    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value {
        if self.config.diff_mode {
            serde_json::to_value(build_prestate_tracer_diff_mode_result(
                emulation_result.states,
            ))
        } else {
            serde_json::to_value(build_prestate_tracer_pre_state(emulation_result.states.pre))
        }
        .expect("Conversion error")
    }
}
