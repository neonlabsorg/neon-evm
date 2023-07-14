use super::{
    trace::{FullTraceData, VMTrace, VMTracer},
    vm_tracer::VmTracer,
};

pub struct Tracer {
    pub vm: VmTracer,
    pub data: Vec<FullTraceData>,
}

impl Default for Tracer {
    fn default() -> Self {
        Tracer::new()
    }
}

impl Tracer {
    #[must_use]
    pub fn new() -> Self {
        Tracer {
            vm: VmTracer::init(),
            data: vec![],
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn into_traces(self) -> (Option<VMTrace>, Vec<FullTraceData>) {
        let vm = self.vm.tracer.drain();
        (vm, self.data)
    }

    #[must_use]
    pub fn copy_traces(&self) -> (Option<VMTrace>, Vec<FullTraceData>) {
        let vm = self.vm.tracer.copy_vm_trace();
        (vm, self.data.clone())
    }
}
