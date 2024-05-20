use crate::tracing::tracers::prestate_tracer::state_diff::{
    build_prestate_tracer_diff_mode_result, build_prestate_tracer_pre_state,
};
use crate::tracing::tracers::state_diff::StateDiffTracer;
use crate::tracing::tracers::Tracer;
use crate::tracing::TraceConfig;
use crate::types::TxParams;
use async_trait::async_trait;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Event, EventListener};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L57>
pub struct PrestateTracer {
    config: PrestateTracerConfig,
    state_diff_tracer: StateDiffTracer,
}

impl PrestateTracer {
    #[must_use]
    pub fn new(trace_config: TraceConfig, tx: &TxParams) -> Self {
        Self {
            config: trace_config.into(),
            state_diff_tracer: StateDiffTracer::new(tx),
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
    #[serde(default)]
    pub diff_mode: bool,
}

#[async_trait(?Send)]
impl EventListener for PrestateTracer {
    async fn event(
        &mut self,
        executor_state: &impl Database,
        event: Event,
    ) -> evm_loader::error::Result<()> {
        self.state_diff_tracer.event(executor_state, event).await
    }
}

impl Tracer for PrestateTracer {
    fn into_traces(self, emulator_gas_used: u64) -> Value {
        let state_map = self.state_diff_tracer.into_state_map(emulator_gas_used);

        if self.config.diff_mode {
            serde_json::to_value(build_prestate_tracer_diff_mode_result(state_map))
        } else {
            serde_json::to_value(build_prestate_tracer_pre_state(state_map))
        }
        .expect("serialization should not fail")
    }
}
