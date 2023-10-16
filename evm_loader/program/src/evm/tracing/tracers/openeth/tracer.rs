use crate::evm::tracing::tracers::openeth::types::{CallAnalytics, TraceResults};
use crate::evm::tracing::{EmulationResult, Event, EventListener};
use serde_json::Value;
use std::fmt::Debug;

#[derive(Debug)]
pub struct OpenEthereumTracer {
    _call_analytics: CallAnalytics,
}

impl OpenEthereumTracer {
    pub fn new(call_analytics: CallAnalytics) -> OpenEthereumTracer {
        OpenEthereumTracer {
            _call_analytics: call_analytics,
        }
    }
}

impl EventListener for OpenEthereumTracer {
    fn event(&mut self, _event: Event) {}

    fn into_traces(self: Box<Self>, _emulation_result: EmulationResult) -> Value {
        serde_json::to_value(TraceResults {
            output: Default::default(),
            trace: vec![],
            vm_trace: None,
            state_diff: None,
        })
        .unwrap()
    }
}
