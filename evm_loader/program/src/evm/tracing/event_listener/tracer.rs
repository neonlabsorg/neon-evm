use std::cell::RefCell;
use std::rc::Rc;

use crate::evm::tracing::event_listener::trace::{FullTraceData, VMTrace, VMTracer};

use super::vm_tracer::VmTracer;

pub type TracerType = Option<Rc<RefCell<Option<Tracer>>>>;

pub struct Tracer {
    pub vm: VmTracer,
    pub data: Vec<FullTraceData>,
    pub enable_return_data: bool,
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
