use std::fmt::Debug;

use serde_json::Value;
use web3::types::Bytes;

use evm_loader::evm::tracing::{EmulationResult, Event, EventListener};

use crate::tracing::tracers::openeth::state_diff::StatesExt;
use crate::tracing::tracers::openeth::types::{CallAnalytics, TraceResults};
use crate::tracing::TraceConfig;

#[derive(Debug)]
pub struct OpenEthereumTracer {
    output: Option<Bytes>,
    call_analytics: CallAnalytics,
}

impl OpenEthereumTracer {
    #[must_use]
    pub fn new(trace_config: TraceConfig) -> OpenEthereumTracer {
        OpenEthereumTracer {
            output: None,
            call_analytics: trace_config.into(),
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

impl EventListener for OpenEthereumTracer {
    fn event(&mut self, event: Event) {
        if let Event::EndStep {
            gas_used: _gas_used,
            return_data,
        } = event
        {
            self.output = return_data.map(Into::into);
        }
    }

    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value {
        serde_json::to_value(TraceResults {
            output: self.output.unwrap_or_default(),
            trace: vec![],
            vm_trace: None,
            state_diff: if self.call_analytics.state_diffing {
                Some(emulation_result.states.into_state_diff())
            } else {
                None
            },
        })
        .expect("Conversion error")
    }
}
