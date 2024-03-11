use crate::tracing::tracers::state_diff::to_web3_u256;
use crate::tracing::tracers::Tracer;
use crate::tracing::TraceConfig;
use crate::types::TxParams;
use async_trait::async_trait;
use evm_loader::error::{format_revert_error, format_revert_panic};
use evm_loader::evm::database::Database;
use evm_loader::evm::opcode_table::Opcode;
use evm_loader::evm::tracing::{Event, EventListener};
use evm_loader::evm::{opcode_table, Context, ExitStatus};
use evm_loader::types::Address;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use web3::types::{Bytes, H256, U256};

pub struct CallTracer {
    config: CallTracerConfig,
    call_stack: Vec<CallFrame>,
    depth: usize,
}

impl CallTracer {
    pub fn new(trace_config: TraceConfig, tx: &TxParams) -> Self {
        CallTracer {
            config: trace_config.into(),
            call_stack: vec![CallFrame {
                gas: tx.gas_limit.map(to_web3_u256).unwrap_or_default(),
                gas_used: tx.actual_gas_used.map(to_web3_u256).unwrap_or_default(),
                ..CallFrame::default()
            }],
            depth: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTracerConfig {
    #[serde(default)]
    pub only_top_call: bool,
    // If true, call tracer won't collect any subcalls
    #[serde(default)]
    pub with_log: bool, // If true, call tracer will collect event logs
}

impl From<TraceConfig> for CallTracerConfig {
    fn from(trace_config: TraceConfig) -> Self {
        let tracer_config = trace_config
            .tracer_config
            .expect("tracer_config should not be None for \"callTracer\"");
        serde_json::from_value(tracer_config).expect("tracer_config should be CallTracerConfig")
    }
}

#[derive(Serialize)]
pub struct CallLog {
    address: Address,
    topics: Vec<H256>,
    data: Bytes,
    // Position of the log relative to subcalls within the same trace
    // See https://github.com/ethereum/go-ethereum/pull/28389 for details
    position: U256,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallFrame {
    from: Address,
    gas: U256,
    gas_used: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<Address>,
    input: Bytes,
    #[serde(skip_serializing_if = "is_empty")]
    output: Bytes,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    revert_reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    calls: Vec<CallFrame>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    logs: Vec<CallLog>,
    // Placed at end on purpose. The RLP will be decoded to 0 instead of
    // nil if there are non-empty elements after in the struct.
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<U256>,
    #[serde(rename = "type")]
    type_string: Opcode,
}

impl CallFrame {
    fn process_output(&mut self, status: ExitStatus) {
        if status.is_succeed().unwrap_or_default() {
            self.output = status.into_result().unwrap_or_default().into();
            return;
        }

        if let ExitStatus::Revert(_) = status {
            self.error = "execution reverted".to_string();
        }

        if self.type_string == opcode_table::CREATE || self.type_string == opcode_table::CREATE2 {
            self.to = None;
        }

        self.output = status.into_result().unwrap_or_default().into();
        self.revert_reason = format_revert_message(&self.output.0);
    }

    // clear_failed_logs clears the logs of a callframe and all its children
    // in case of execution failure.
    fn clear_failed_logs(&mut self, parent_failed: bool) {
        let failed = !self.error.is_empty() || parent_failed;
        if failed {
            self.logs.clear();
        }
        for call in &mut self.calls {
            call.clear_failed_logs(failed);
        }
    }
}

fn format_revert_message(msg: &[u8]) -> String {
    if let Some(reason) = format_revert_error(msg) {
        return reason.to_string();
    }

    if let Some(reason) = format_revert_panic(msg) {
        return get_panic_message(reason);
    }

    String::new()
}

fn get_panic_message(reason: ethnum::U256) -> String {
    let reason = reason.as_u64();
    PANIC_REASONS
        .get(&reason)
        .map(|s| s.to_string())
        .unwrap_or(format!("unknown panic code: {reason:#x}"))
}

lazy_static! {
    // panic_reasons map is for readable panic codes
    // see this linkage for the details
    // https://docs.soliditylang.org/en/v0.8.21/control-structures.html#panic-via-assert-and-error-via-require
    // the reason string list is copied from ethers.js
    // https://github.com/ethers-io/ethers.js/blob/fa3a883ff7c88611ce766f58bdd4b8ac90814470/src.ts/abi/interface.ts#L207-L218
    static ref PANIC_REASONS: HashMap<u64, &'static str> = HashMap::from([
        (0x00, "generic panic"),
        (0x01, "assert(false)"),
        (0x11, "arithmetic underflow or overflow"),
        (0x12, "division or modulo by zero"),
        (0x21, "enum overflow"),
        (0x22, "invalid encoded storage byte array accessed"),
        (0x31, "out-of-bounds array access; popping on an empty array"),
        (0x32, "out-of-bounds access of an array or bytesN"),
        (0x41, "out of memory"),
        (0x51, "uninitialized function"),
    ]);
}

fn is_empty(bytes: &Bytes) -> bool {
    bytes.0.is_empty()
}

#[async_trait(?Send)]
impl EventListener for CallTracer {
    async fn event(
        &mut self,
        _executor_state: &impl Database,
        event: Event,
    ) -> evm_loader::error::Result<()> {
        match event {
            Event::BeginVM {
                context,
                opcode,
                input,
                ..
            } => {
                self.depth += 1;
                self.handle_begin_vm(context, opcode, input);
            }
            Event::EndVM { status, .. } => {
                self.handle_end_vm(status);
                self.depth -= 1;
            }
            Event::BeginStep {
                context,
                opcode,
                stack,
                memory,
                ..
            } => {
                // Only logs need to be captured via opcode processing
                if !self.config.with_log {
                    return Ok(());
                }

                // Avoid processing nested calls when only caring about top call
                if self.config.only_top_call && self.depth > 1 {
                    return Ok(());
                }

                match opcode {
                    opcode_table::LOG0
                    | opcode_table::LOG1
                    | opcode_table::LOG2
                    | opcode_table::LOG3
                    | opcode_table::LOG4 => {
                        let size = (opcode.0 - opcode_table::LOG0.0) as usize;

                        let m_start = U256::from(stack[stack.len() - 1]).as_usize();
                        let m_size = U256::from(stack[stack.len() - 2]).as_usize();

                        let mut topics = Vec::with_capacity(size);

                        for i in 0..size {
                            topics.push(H256::from(stack[stack.len() - 2 - (i + 1)]));
                        }

                        let call_log = CallLog {
                            address: context.contract,
                            topics,
                            data: memory[m_start..m_start + m_size].to_vec().into(),
                            position: self.call_stack.last().unwrap().calls.len().into(),
                        };

                        self.call_stack.last_mut().unwrap().logs.push(call_log);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

impl CallTracer {
    fn handle_begin_vm(&mut self, context: Context, opcode: Opcode, input: Vec<u8>) {
        if self.depth == 1 {
            let call_frame = &mut self.call_stack[0];
            call_frame.from = context.caller;
            call_frame.to = Some(context.contract);
            call_frame.input = input.into();
            call_frame.value = Some(to_web3_u256(context.value));
            call_frame.type_string = opcode;
            return;
        }

        if self.config.only_top_call {
            return;
        }

        self.call_stack.push(CallFrame {
            from: context.caller,
            to: Some(context.contract),
            input: input.into(),
            value: Some(to_web3_u256(context.value)),
            type_string: opcode,
            ..CallFrame::default()
        })
    }

    fn handle_end_vm(&mut self, status: ExitStatus) {
        if self.depth == 1 {
            self.call_stack[0].process_output(status);
            if self.config.with_log {
                self.call_stack[0].clear_failed_logs(false);
            }
            return;
        }

        if self.config.only_top_call {
            return;
        }

        if self.call_stack.len() <= 1 {
            return;
        }

        let mut call_frame = self.call_stack.pop().unwrap();

        call_frame.process_output(status);

        self.call_stack.last_mut().unwrap().calls.push(call_frame);
    }
}

impl Tracer for CallTracer {
    fn into_traces(mut self, emulator_gas_used: u64) -> Value {
        if self.call_stack.len() != 1 {
            panic!("incorrect number of top-level calls");
        }

        let call_frame = &mut self.call_stack[0];
        if call_frame.gas_used.is_zero() {
            call_frame.gas_used = U256::from(emulator_gas_used);
        }

        serde_json::to_value(call_frame).expect("serialization should not fail")
    }
}
