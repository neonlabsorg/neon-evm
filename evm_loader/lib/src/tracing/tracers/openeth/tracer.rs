use async_trait::async_trait;
use ethnum::U256;
use std::fmt::Debug;

use evm_loader::evm::database::Database;
use serde_json::Value;
use web3::types::Bytes;

use evm_loader::evm::tracing::{EmulationResult, Event, EventListener};

use crate::tracing::tracers::openeth::types::{CallAnalytics, TraceResults};
use crate::tracing::tracers::state_diff::StateDiffTracer;
use crate::tracing::TraceConfig;

#[derive(Debug)]
pub struct OpenEthereumTracer {
    output: Option<Bytes>,
    call_analytics: CallAnalytics,
    state_diff_tracer: StateDiffTracer,
}

impl OpenEthereumTracer {
    #[must_use]
    pub fn new(tx_fee: U256, trace_config: TraceConfig) -> Self {
        OpenEthereumTracer {
            output: None,
            call_analytics: trace_config.into(),
            state_diff_tracer: StateDiffTracer {
                tx_fee: web3::types::U256::from(tx_fee.to_be_bytes()),
                ..StateDiffTracer::default()
            },
        }
    }
}

impl From<TraceConfig> for CallAnalytics {
    fn from(trace_config: TraceConfig) -> Self {
        let tracer_config = trace_config
            .tracer_config
            .expect("tracer_config should not be None for \"openethereum\" tracer");
        serde_json::from_value(tracer_config).expect("tracer_config should be CallAnalytics")
    }
}

#[async_trait(?Send)]
impl EventListener for OpenEthereumTracer {
    async fn event(
        &mut self,
        executor_state: &mut impl Database,
        event: Event,
        chain_id: u64,
    ) -> evm_loader::error::Result<()> {
        if let Event::EndStep {
            gas_used: _gas_used,
            return_data,
        } = &event
        {
            self.output = return_data.clone().map(Into::into);
        }
        self.state_diff_tracer
            .event(executor_state, event, chain_id)
            .await
    }

    fn into_traces(self, _emulation_result: EmulationResult) -> Value {
        serde_json::to_value(TraceResults {
            output: self.output.unwrap_or_default(),
            trace: vec![],
            vm_trace: None,
            state_diff: if self.call_analytics.state_diffing {
                Some(self.state_diff_tracer.states.into_state_diff())
            } else {
                None
            },
        })
        .expect("Conversion error")
    }
}
