use crate::evm::Buffer;
use crate::types::hexbytes::HexBytes;
use ethnum::U256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
/// A diff of some chunk of memory.
pub struct MemoryDiff {
    /// Offset into memory the change begins.
    pub offset: usize,
    /// The changed data.
    pub data: HexBytes,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
/// A diff of some storage value.
pub struct StorageDiff {
    /// Which key in storage is changed.
    pub location: U256,
    /// What the value has been changed to.
    pub value: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
/// A record of an executed VM operation.
pub struct VMExecutedOperation {
    /// The total gas used.
    pub gas_used: U256,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
/// A record of the execution of a single VM operation.
pub struct VMOperation {
    /// The program counter.
    pub pc: usize,
    /// The instruction executed.
    pub instruction: u8,
    /// The gas cost for this instruction.
    pub gas_cost: U256,
    /// Information concerning the execution of the operation.
    pub executed: Option<VMExecutedOperation>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
/// A record of a full VM trace for a CALL/CREATE.
#[allow(clippy::module_name_repetitions)]
pub struct VMTrace {
    /// The step (i.e. index into operations) at which this trace corresponds.
    pub parent_step: usize,
    /// The code to be executed.
    pub code: HexBytes,
    /// The operations executed.
    pub operations: Vec<VMOperation>,
    /// The sub traces for each interior action performed as part of this call/create.
    /// There is a 1:1 correspondence between these and a CALL/CREATE/CALLCODE/DELEGATECALL instruction.
    pub subs: Vec<VMTrace>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FullTraceData {
    pub stack: Vec<[u8; 32]>,
    pub memory: Vec<u8>,
    pub storage: HashMap<U256, [u8; 32]>,
    pub return_data: Option<Arc<Buffer>>,
}

/// Simple VM tracer. Traces all operations.
#[derive(Debug)]
pub struct ExecutiveVMTracer {
    data: VMTrace,
    pub depth: usize,
}

impl ExecutiveVMTracer {
    /// Create a new top-level instance.
    #[must_use]
    pub fn toplevel() -> Self {
        ExecutiveVMTracer {
            data: VMTrace {
                parent_step: 0,
                code: HexBytes::default(),
                operations: vec![VMOperation::default()], // prefill with a single entry so that prepare_subtrace can get the parent_step
                subs: vec![],
            },
            depth: 0,
        }
    }

    fn with_trace_in_depth<F: FnOnce(&mut VMTrace)>(trace: &mut VMTrace, depth: usize, f: F) {
        if depth == 0 {
            f(trace);
        } else {
            Self::with_trace_in_depth(trace.subs.last_mut().expect("self.depth is incremented with prepare_subtrace; a subtrace is always pushed; self.depth cannot be greater than subtrace stack; qed"), depth - 1, f);
        }
    }
}

// ethcore/src/trace/mod.rs
pub trait VMTracer: Send {
    /// Data returned when draining the `VMTracer`.
    type Output;

    /// Trace the preparation to execute a single valid instruction.
    fn trace_prepare_execute(&mut self, _pc: usize, _instruction: u8) {}

    /// Trace the finalised execution of a single valid instruction.
    fn trace_executed(&mut self, _gas_used: U256) {}

    /// Spawn subtracer which will be used to trace deeper levels of execution.
    fn prepare_subtrace(&mut self, _code: Arc<Buffer>) {}

    /// Finalize subtracer.
    fn done_subtrace(&mut self) {}

    /// Consumes self and returns the VM trace.
    fn drain(self) -> Option<Self::Output>;
}

impl VMTracer for ExecutiveVMTracer {
    type Output = VMTrace;

    fn trace_prepare_execute(&mut self, pc: usize, instruction: u8) {
        Self::with_trace_in_depth(&mut self.data, self.depth, move |trace| {
            trace.operations.push(VMOperation {
                pc,
                instruction,
                gas_cost: U256::ZERO,
                executed: None,
            });
        });
    }

    fn trace_executed(&mut self, gas_used: U256) {
        Self::with_trace_in_depth(&mut self.data, self.depth, move |trace| {
            let operation = trace.operations.last_mut().expect("trace_executed is always called after a trace_prepare_execute; trace.operations cannot be empty; qed");
            operation.executed = Some(VMExecutedOperation { gas_used });
        });
    }

    fn prepare_subtrace(&mut self, code: Arc<Buffer>) {
        Self::with_trace_in_depth(&mut self.data, self.depth, move |trace| {
            let parent_step = trace.operations.len() - 1; // won't overflow since we must already have pushed an operation in trace_prepare_execute.
            trace.subs.push(VMTrace {
                parent_step,
                code: code.to_vec().into(),
                operations: vec![],
                subs: vec![],
            });
        });
        self.depth += 1;
    }

    fn done_subtrace(&mut self) {
        self.depth -= 1;
    }

    fn drain(mut self) -> Option<VMTrace> {
        self.data.subs.pop()
    }
}

#[derive(Debug, Default, Clone)]
pub struct StepDiff {
    pub storage_access: Option<(U256, [u8; 32])>,
    pub storage_set: Option<StorageDiff>,
    pub memory_set: Option<MemoryDiff>,
    pub stack_push: Vec<[u8; 32]>,
}

#[derive(Debug)]
pub struct VmTracer {
    pub tracer: ExecutiveVMTracer,
    step_diff: Vec<StepDiff>,
}

impl VmTracer {
    pub fn init() -> Self {
        VmTracer {
            tracer: ExecutiveVMTracer::toplevel(),
            step_diff: Vec::new(),
        }
    }

    pub fn push_step_diff(&mut self) {
        self.step_diff.push(StepDiff::default());
    }

    pub fn pop_step_diff(&mut self) {
        self.step_diff.pop();
    }

    pub fn step_diff(&mut self) -> &mut StepDiff {
        self.step_diff
            .last_mut()
            .expect("diff was pushed in begin_vm")
    }
}
