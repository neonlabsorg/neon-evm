use crate::account::EthereumAccount;
use crate::evm::Buffer;
use crate::types::hexbytes::HexBytes;
use crate::types::Address;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use {ethnum::U256, std::collections::HashMap};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// A diff of some chunk of memory.
pub struct MemoryDiff {
    /// Offset into memory the change begins.
    pub offset: usize,
    /// The changed data.
    pub data: HexBytes,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// A diff of some storage value.
pub struct StorageDiff {
    /// Which key in storage is changed.
    pub location: U256,
    /// What the value has been changed to.
    pub value: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// A record of an executed VM operation.
pub struct VMExecutedOperation {
    /// The total gas used.
    pub gas_used: U256,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
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
    /// Thre is a 1:1 correspondance between these and a CALL/CREATE/CALLCODE/DELEGATECALL instruction.
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockOverrides {
    pub number: Option<u64>,
    #[allow(unused)]
    pub difficulty: Option<U256>, // NOT SUPPORTED by Neon EVM
    pub time: Option<i64>,
    #[allow(unused)]
    pub gas_limit: Option<u64>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub coinbase: Option<Address>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub random: Option<U256>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub base_fee: Option<U256>, // NOT SUPPORTED BY Neon EVM
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOverride {
    pub nonce: Option<u64>,
    pub code: Option<HexBytes>,
    pub balance: Option<U256>,
    pub state: Option<HashMap<U256, U256>>,
    pub state_diff: Option<HashMap<U256, U256>>,
}

impl AccountOverride {
    pub fn apply(&self, ether_account: &mut EthereumAccount) {
        if let Some(nonce) = self.nonce {
            ether_account.trx_count = nonce;
        }
        if let Some(balance) = self.balance {
            ether_account.balance = U256::from(balance);
        }
        #[allow(clippy::cast_possible_truncation)]
        if let Some(code) = &self.code {
            ether_account.code_size = code.len() as u32;
        }
    }
}

pub type AccountOverrides = HashMap<Address, AccountOverride>;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions, clippy::struct_excessive_bools)]
pub struct TraceConfig {
    #[serde(default)]
    pub enable_memory: bool,
    #[serde(default)]
    pub disable_storage: bool,
    #[serde(default)]
    pub disable_stack: bool,
    #[serde(default)]
    pub enable_return_data: bool,
    pub tracer: Option<String>,
    pub timeout: Option<String>,
    pub tracer_config: Value,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions)]
pub struct TraceCallConfig {
    #[serde(flatten)]
    pub trace_config: TraceConfig,
    pub block_overrides: Option<BlockOverrides>,
    pub state_overrides: Option<AccountOverrides>,
}

impl From<TraceConfig> for TraceCallConfig {
    fn from(trace_config: TraceConfig) -> Self {
        Self {
            trace_config,
            ..Self::default()
        }
    }
}
