use crate::account_storage::ProgramAccountStorage;
use crate::evm::{Buffer, Machine};
use ethnum::U256;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::iter;
use std::sync::Arc;
use vm_tracer::VmTracer;

use crate::evm::tracing::trace::{FullTraceData, TraceConfig, VMOperation, VMTrace, VMTracer};
use crate::evm::tracing::tracers::struct_logger::listener_vm_tracer::ListenerVmTracer;
use crate::evm::tracing::{EmulationResult, Event, EventListener};
use crate::executor::ExecutorState;
use crate::types::hexbytes::HexBytes;

pub mod listener_vm_tracer;
mod vm_tracer;

/// `StructLoggerResult` groups all structured logs emitted by the EVM
/// while replaying a transaction in debug mode as well as transaction
/// execution status, the amount of gas used and the return value
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StructLoggerResult {
    /// Is execution failed or not
    pub failed: bool,
    /// Total used gas but include the refunded gas
    pub gas: u64,
    /// The data after execution or revert reason
    pub return_value: String,
    /// Logs emitted during execution
    pub struct_logs: Vec<StructLog>,
}

/// `StructLog` stores a structured log emitted by the EVM while replaying a
/// transaction in debug mode
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StructLog {
    /// Program counter.
    pub pc: u64,
    /// Operation name
    #[serde(rename(serialize = "op"))]
    pub op_name: &'static str,
    /// Amount of used gas
    pub gas: Option<u64>,
    /// Gas cost for this instruction.
    pub gas_cost: u64,
    /// Current depth
    pub depth: usize,
    /// Snapshot of the current memory sate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<Vec<HexBytes>>, // U256 sized chunks
    /// Snapshot of the current stack sate
    #[serde(skip_serializing_if = "Option::is_none")]
    // pub stack: Option<Vec<[u8; 32]>>,
    pub stack: Option<Vec<U256>>,
    /// Result of the step
    pub return_data: Option<Arc<Buffer>>,
    /// Snapshot of the current storage
    #[serde(skip_serializing_if = "Option::is_none")]
    // pub storage: Option<BTreeMap<U256, [u8; 32]>>,
    pub storage: Option<BTreeMap<U256, U256>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StructLog {
    // use boxing bc of the recursive opaque type
    fn from_trace_with_depth(vm_trace: VMTrace, depth: usize) -> Box<dyn Iterator<Item = Self>> {
        let operations = vm_trace.operations;
        let mut subs = vm_trace.subs.into_iter().peekable();

        Box::new(
            operations
                .into_iter()
                .enumerate()
                .flat_map(move |(idx, operation)| {
                    let main_op = iter::once((depth, operation).into());
                    let subtrace_iter = if subs
                        .peek()
                        .map_or(false, |subtrace| idx == subtrace.parent_step)
                    {
                        let subtrace = subs.next().expect("just peeked it");
                        Some(Self::from_trace_with_depth(subtrace, depth + 1))
                    } else {
                        None
                    };
                    main_op.chain(subtrace_iter.into_iter().flatten())
                }),
        )
    }
}

impl From<(usize, VMOperation)> for StructLog {
    fn from((depth, vm_operation): (usize, VMOperation)) -> Self {
        let pc = vm_operation.pc as u64;
        let (op_name, _) = Machine::<ExecutorState<ProgramAccountStorage>>::OPCODES
            [vm_operation.instruction as usize];
        let gas = vm_operation.executed.as_ref().map(|e| e.gas_used.as_u64());
        let gas_cost = vm_operation.gas_cost.as_u64();
        let depth = depth;
        let memory = None;
        let stack = None;
        let return_data = None;
        let storage = None;
        let error = None;

        Self {
            pc,
            op_name,
            gas,
            gas_cost,
            depth,
            memory,
            stack,
            return_data,
            storage,
            error,
        }
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct StructLogger {
    pub vm: VmTracer,
    pub data: Vec<FullTraceData>,
    pub enable_memory: bool,
    pub disable_storage: bool,
    pub disable_stack: bool,
    pub enable_return_data: bool,
}

impl StructLogger {
    #[must_use]
    pub fn new(trace_config: &TraceConfig) -> Self {
        StructLogger {
            vm: VmTracer::init(),
            data: vec![],
            enable_memory: trace_config.enable_memory,
            disable_storage: trace_config.disable_storage,
            disable_stack: trace_config.disable_stack,
            enable_return_data: trace_config.enable_return_data,
        }
    }
}

pub trait ListenerTracer {
    fn begin_step(&mut self, stack: Vec<[u8; 32]>, memory: Vec<u8>);
    fn end_step(&mut self, return_data: Option<Arc<Buffer>>);
}

impl ListenerTracer for StructLogger {
    fn begin_step(&mut self, stack: Vec<[u8; 32]>, memory: Vec<u8>) {
        let storage = self
            .data
            .last()
            .map(|d| d.storage.clone())
            .unwrap_or_default();

        self.data.push(FullTraceData {
            stack,
            memory,
            storage,
            return_data: None,
        });
    }

    fn end_step(&mut self, return_data: Option<Arc<Buffer>>) {
        let data = self
            .data
            .last_mut()
            .expect("No data were pushed in `begin_step`");
        data.return_data = return_data;
        if let Some((index, value)) = self.vm.step_diff().storage_access {
            data.storage.insert(index, value);
        }
    }
}

impl EventListener for StructLogger {
    fn event(&mut self, event: Event) {
        match event {
            Event::BeginVM { context, code } => {
                self.vm.begin_vm(context, code);
            }
            Event::EndVM { status } => {
                self.vm.end_vm(status);
            }
            Event::BeginStep {
                opcode,
                pc,
                stack,
                memory,
            } => {
                self.begin_step(stack, memory);
                self.vm.begin_step(opcode, pc);
            }
            Event::EndStep {
                gas_used,
                return_data,
            } => {
                self.end_step(if self.enable_return_data {
                    return_data
                } else {
                    None
                });
                self.vm.end_step(gas_used);
            }
            Event::StackPush { value } => {
                self.vm.stack_push(value);
            }
            Event::MemorySet { offset, data } => {
                self.vm.memory_set(offset, data);
            }
            Event::StorageSet { index, value } => {
                self.vm.storage_set(index, value);
            }
            Event::StorageAccess { index, value } => {
                self.vm.storage_access(index, value);
            }
        };
    }

    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value {
        let mut logs: Vec<StructLog> = match self.vm.tracer.drain() {
            Some(vm_trace) => StructLog::from_trace_with_depth(vm_trace, 1).collect(),
            None => vec![],
        };

        assert_eq!(logs.len(), self.data.len());

        logs.iter_mut()
            .zip(self.data.into_iter())
            .for_each(|(l, d)| {
                if !self.disable_stack {
                    l.stack = Some(
                        d.stack
                            .iter()
                            .map(|entry| U256::from_be_bytes(*entry))
                            .collect(),
                    );
                }

                if self.enable_memory && !d.memory.is_empty() {
                    l.memory = Some(
                        d.memory
                            .chunks(32)
                            .map(|slice| slice.to_vec().into())
                            .collect(),
                    );
                }

                if !self.disable_storage {
                    l.storage = Some(
                        d.storage
                            .into_iter()
                            .map(|(k, v)| (k, U256::from_be_bytes(v)))
                            .collect(),
                    );
                }

                if self.enable_return_data {
                    l.return_data = d.return_data;
                }
            });

        let result = StructLoggerResult {
            failed: !emulation_result
                .exit_status
                .is_succeed()
                .expect("Emulation is not completed"),
            gas: emulation_result.used_gas,
            return_value: hex::encode(
                emulation_result
                    .exit_status
                    .into_result()
                    .unwrap_or_default(),
            ),
            struct_logs: logs,
        };

        serde_json::to_value(result).expect("Conversion error")
    }
}
