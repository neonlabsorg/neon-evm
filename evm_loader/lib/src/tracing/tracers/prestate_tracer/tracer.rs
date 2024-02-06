use crate::tracing::tracers::prestate_tracer::state_diff::{
    build_prestate_tracer_diff_mode_result, build_prestate_tracer_pre_state,
};
use crate::tracing::tracers::state_diff::StateDiffTracer;
use crate::tracing::TraceConfig;
use async_trait::async_trait;
use ethnum::U256;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{EmulationResult, Event, EventListener};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L57>
#[derive(Debug)]
pub struct PrestateTracer {
    config: PrestateTracerConfig,
    state_diff_tracer: StateDiffTracer,
}

impl PrestateTracer {
    pub fn new(tx_fee: U256, trace_config: TraceConfig) -> Self {
        PrestateTracer {
            config: trace_config.into(),
            state_diff_tracer: StateDiffTracer {
                tx_fee: web3::types::U256::from(tx_fee.to_be_bytes()),
                ..StateDiffTracer::default()
            },
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

#[async_trait(?Send)]
impl EventListener for PrestateTracer {
    async fn event(
        &mut self,
        executor_state: &mut impl Database,
        event: Event,
        chain_id: u64,
    ) -> evm_loader::error::Result<()> {
        self.state_diff_tracer
            .event(executor_state, event, chain_id)
            .await
    }

    fn into_traces(self, _emulation_result: EmulationResult) -> Value {
        if self.config.diff_mode {
            serde_json::to_value(build_prestate_tracer_diff_mode_result(
                self.state_diff_tracer.states,
            ))
        } else {
            serde_json::to_value(build_prestate_tracer_pre_state(
                self.state_diff_tracer.states.pre,
            ))
        }
        .expect("Conversion error")
    }
}
