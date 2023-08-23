use crate::evm::tracing::trace::{MemoryDiff, StorageDiff, VMTracer};
use crate::evm::{Buffer, Context, ExitStatus};
use ethnum::U256;
use std::sync::Arc;

use super::vm_tracer::VmTracer;

pub trait ListenerVmTracer {
    fn begin_vm(&mut self, context: Context, code: Arc<Buffer>);
    fn end_vm(&mut self, status: ExitStatus);
    fn begin_step(&mut self, opcode: u8, pc: usize);
    fn end_step(&mut self, gas_used: u64);
    fn storage_access(&mut self, index: U256, value: [u8; 32]);
    fn storage_set(&mut self, index: U256, value: [u8; 32]);
    fn stack_push(&mut self, value: [u8; 32]);
    fn memory_set(&mut self, offset: usize, data: Vec<u8>);
}

impl ListenerVmTracer for VmTracer {
    fn begin_vm(&mut self, _context: Context, code: Arc<Buffer>) {
        self.push_step_diff();

        self.tracer.prepare_subtrace(code);
    }

    fn end_vm(&mut self, _status: ExitStatus) {
        self.pop_step_diff();

        self.tracer.done_subtrace();
    }

    fn begin_step(&mut self, opcode: u8, pc: usize) {
        let diff = self.step_diff();
        diff.stack_push.clear();
        diff.memory_set = None;
        diff.storage_set = None;
        diff.storage_access = None;

        self.tracer.trace_prepare_execute(pc, opcode);
    }

    fn end_step(&mut self, gas_used: u64) {
        let gas_used = U256::from(gas_used);

        self.tracer.trace_executed(gas_used);
    }

    fn storage_access(&mut self, index: U256, value: [u8; 32]) {
        self.step_diff().storage_access = Some((index, value));
    }

    fn storage_set(&mut self, index: U256, value: [u8; 32]) {
        self.step_diff().storage_set = Some(StorageDiff {
            location: index,
            value,
        });
    }

    fn stack_push(&mut self, value: [u8; 32]) {
        self.step_diff().stack_push.push(value);
    }

    fn memory_set(&mut self, offset: usize, data: Vec<u8>) {
        self.step_diff().memory_set = Some(MemoryDiff {
            offset,
            data: data.into(),
        });
    }
}
