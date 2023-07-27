use crate::event_listener::trace::VMTracer;
use crate::event_listener::trace::{FullTraceData, VMTrace};

use super::vm_tracer::VmTracer;

pub struct Tracer {
    pub vm: VmTracer,
    pub data: Vec<FullTraceData>,
    pub(crate) enable_return_data: bool,
}

impl Tracer {
    pub fn new(enable_return_data: bool) -> Self {
        Tracer {
            vm: VmTracer::init(),
            data: vec![],
            enable_return_data,
        }
    }

    pub fn into_traces(self) -> (Option<VMTrace>, Vec<FullTraceData>) {
        let vm = self.vm.tracer.drain();
        (vm, self.data)
    }
}
