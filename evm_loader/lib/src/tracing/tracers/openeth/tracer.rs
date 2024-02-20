use async_trait::async_trait;

use evm_loader::evm::database::Database;
use serde_json::Value;
use web3::types::Bytes;

use evm_loader::evm::tracing::{Event, EventListener};

use crate::tracing::tracers::openeth::state_diff::into_state_diff;
use crate::tracing::tracers::openeth::types::{CallAnalytics, TraceResults};
use crate::tracing::tracers::state_diff::StateDiffTracer;
use crate::tracing::tracers::Tracer;
use crate::tracing::TraceConfig;
use crate::types::TxParams;

pub struct OpenEthereumTracer {
    output: Option<Bytes>,
    call_analytics: CallAnalytics,
    state_diff_tracer: StateDiffTracer,
}

impl OpenEthereumTracer {
    #[must_use]
    pub fn new(trace_config: TraceConfig, tx: &TxParams) -> Self {
        OpenEthereumTracer {
            output: None,
            call_analytics: trace_config.into(),
            state_diff_tracer: StateDiffTracer::new(tx),
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
        executor_state: &impl Database,
        event: Event,
    ) -> evm_loader::error::Result<()> {
        if let Event::EndVM { status, .. } = &event {
            self.output = status.clone().into_result().map(Into::into);
        }
        self.state_diff_tracer.event(executor_state, event).await
    }
}

impl Tracer for OpenEthereumTracer {
    fn into_traces(self, _used_gas: u64) -> Value {
        serde_json::to_value(TraceResults {
            output: self.output.unwrap_or_default(),
            trace: vec![],
            vm_trace: None,
            state_diff: if self.call_analytics.state_diffing {
                Some(into_state_diff(self.state_diff_tracer.state_map))
            } else {
                None
            },
        })
        .expect("serialization should not fail")
    }
}
