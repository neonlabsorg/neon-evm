use crate::evm::tracing::tracers::openeth::types::{CallAnalytics, TraceResults};
use crate::evm::tracing::{EmulationResult, Event, EventListener};
use crate::types::hexbytes::HexBytes;
use serde_json::Value;
use std::fmt::Debug;

#[derive(Debug)]
pub struct OpenEthereumTracer {
    output: Option<HexBytes>,
    call_analytics: CallAnalytics,
}

impl OpenEthereumTracer {
    #[must_use]
    pub fn new(call_analytics: CallAnalytics) -> OpenEthereumTracer {
        OpenEthereumTracer {
            output: None,
            call_analytics,
        }
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
                Some(emulation_result.state_diff)
            } else {
                None
            },
        })
        .unwrap()
    }
}
