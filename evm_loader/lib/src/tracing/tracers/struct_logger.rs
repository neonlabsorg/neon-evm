use std::collections::BTreeMap;

use ethnum::U256;
use serde::Serialize;
use serde_json::Value;
use web3::types::Bytes;

use evm_loader::evm::opcode_table::OPNAMES;
use evm_loader::evm::tracing::{Event, EventListener};
use evm_loader::evm::ExitStatus;

use crate::tracing::tracers::{EmulationResult, IntoTraces};
use crate::tracing::TraceConfig;

/// `StructLoggerResult` groups all structured logs emitted by the EVM
/// while replaying a transaction in debug mode as well as transaction
/// execution status, the amount of gas used and the return value
/// see <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/logger/logger.go#L404>
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StructLoggerResult {
    /// Total used gas but include the refunded gas
    pub gas: u64,
    /// Is execution failed or not
    pub failed: bool,
    /// The data after execution or revert reason
    pub return_value: String,
    /// Logs emitted during execution
    pub struct_logs: Vec<StructLog>,
}

/// `StructLog` stores a structured log emitted by the EVM while replaying a
/// transaction in debug mode
/// see <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/logger/logger.go#L413>
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StructLog {
    /// Program counter.
    pc: u64,
    /// Operation name
    op: &'static str,
    /// Amount of used gas
    gas: u64,
    /// Gas cost for this instruction.
    gas_cost: u64,
    /// Current depth
    depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Snapshot of the current stack sate
    #[serde(skip_serializing_if = "Option::is_none")]
    stack: Option<Vec<U256>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_data: Option<Bytes>,
    /// Snapshot of the current memory sate
    #[serde(skip_serializing_if = "Option::is_none")]
    memory: Option<Vec<String>>, // chunks of 32 bytes
    /// Result of the step
    /// Snapshot of the current storage
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<BTreeMap<String, String>>,
    /// Refund counter
    #[serde(skip_serializing_if = "is_zero")]
    refund: u64,
}

/// This is only used for serialize
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(num: &u64) -> bool {
    *num == 0
}

impl StructLog {
    #[must_use]
    pub fn new(
        opcode: u8,
        pc: u64,
        gas_cost: u64,
        depth: usize,
        memory: Option<Vec<String>>,
        stack: Option<Vec<U256>>,
    ) -> Self {
        let op = OPNAMES[opcode as usize];
        Self {
            pc,
            op,
            gas: 0,
            gas_cost,
            depth,
            memory,
            stack,
            return_data: None,
            storage: None,
            error: None,
            refund: 0,
        }
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
struct Config {
    enable_memory: bool,
    disable_storage: bool,
    disable_stack: bool,
    enable_return_data: bool,
}

impl From<TraceConfig> for Config {
    fn from(trace_config: TraceConfig) -> Self {
        Self {
            enable_memory: trace_config.enable_memory,
            disable_storage: trace_config.disable_storage,
            disable_stack: trace_config.disable_stack,
            enable_return_data: trace_config.enable_return_data,
        }
    }
}

#[derive(Debug)]
pub struct StructLogger {
    gas_used: Option<U256>,
    config: Config,
    logs: Vec<StructLog>,
    depth: usize,
    storage_access: Option<(U256, U256)>,
    exit_status: Option<ExitStatus>,
}

impl StructLogger {
    #[must_use]
    pub fn new(gas_used: Option<U256>, trace_config: TraceConfig) -> Self {
        StructLogger {
            gas_used,
            config: trace_config.into(),
            logs: vec![],
            depth: 0,
            storage_access: None,
            exit_status: None,
        }
    }
}

impl EventListener for StructLogger {
    fn event(&mut self, event: Event) {
        match event {
            Event::BeginVM { .. } => {
                self.depth += 1;
            }
            Event::EndVM { status } => {
                self.exit_status = Some(status);
                self.depth -= 1;
            }
            Event::BeginStep {
                opcode,
                pc,
                stack,
                memory,
            } => {
                let stack = if self.config.disable_stack {
                    None
                } else {
                    Some(
                        stack
                            .iter()
                            .map(|entry| U256::from_be_bytes(*entry))
                            .collect(),
                    )
                };

                let memory = if self.config.enable_memory {
                    Some(memory.chunks(32).map(hex::encode).collect())
                } else {
                    None
                };

                let log = StructLog::new(opcode, pc as u64, 0, self.depth, memory, stack);
                self.logs.push(log);
            }
            Event::EndStep {
                gas_used,
                return_data,
            } => {
                let last = self
                    .logs
                    .last_mut()
                    .expect("`EndStep` event before `BeginStep`");
                last.gas = gas_used;
                if !self.config.disable_storage {
                    if let Some((index, value)) = self.storage_access.take() {
                        last.storage.get_or_insert_with(Default::default).insert(
                            hex::encode(index.to_be_bytes()),
                            hex::encode(value.to_be_bytes()),
                        );
                    };
                }
                if self.config.enable_return_data {
                    last.return_data = return_data.map(Into::into);
                }
            }
            Event::StorageGet {
                address: _,
                index,
                value,
            }
            | Event::StorageSet {
                address: _,
                index,
                value,
            } => {
                if !self.config.disable_storage {
                    self.storage_access = Some((index, U256::from_be_bytes(value)));
                }
            }
        };
    }
}

impl IntoTraces for StructLogger {
    fn into_traces(self, emulation_result: EmulationResult) -> Value {
        let exit_status = self.exit_status.expect("Exit status should be set");
        let result = StructLoggerResult {
            failed: !exit_status
                .is_succeed()
                .expect("Emulation is not completed"),
            gas: self
                .gas_used
                .map_or(emulation_result.used_gas, U256::as_u64),
            return_value: hex::encode(exit_status.into_result().unwrap_or_default()),
            struct_logs: self.logs,
        };

        serde_json::to_value(result).expect("Conversion error")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_struct_logger_result_all_fields() {
        let struct_logger_result = StructLoggerResult {
            gas: 20000,
            failed: false,
            return_value: "000000000000000000000000000000000000000000000000000000000000001b"
                .to_string(),
            struct_logs: vec![StructLog {
                pc: 8,
                op: "PUSH2",
                gas: 0,
                gas_cost: 0,
                depth: 1,
                stack: Some(vec![U256::from(0u8), U256::from(1u8)]),
                memory: Some(vec![
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                    "0000000000000000000000000000000000000000000000000000000000000080".to_string(),
                ]),
                return_data: None,
                storage: None,
                refund: 0,
                error: None,
            }],
        };
        assert_eq!(serde_json::to_string(&struct_logger_result).unwrap(), "{\"gas\":20000,\"failed\":false,\"returnValue\":\"000000000000000000000000000000000000000000000000000000000000001b\",\"structLogs\":[{\"pc\":8,\"op\":\"PUSH2\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[\"0x0\",\"0x1\"],\"memory\":[\"0000000000000000000000000000000000000000000000000000000000000000\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"0000000000000000000000000000000000000000000000000000000000000080\"]}]}");
    }

    #[test]
    fn test_serialize_struct_logger_result_no_optional_fields() {
        let struct_logger_result = StructLoggerResult {
            gas: 20000,
            failed: false,
            return_value: "000000000000000000000000000000000000000000000000000000000000001b"
                .to_string(),
            struct_logs: vec![StructLog {
                pc: 0,
                op: "PUSH1",
                gas: 0,
                gas_cost: 0,
                depth: 1,
                stack: None,
                memory: None,
                return_data: None,
                storage: None,
                refund: 0,
                error: None,
            }],
        };
        assert_eq!(serde_json::to_string(&struct_logger_result).unwrap(), "{\"gas\":20000,\"failed\":false,\"returnValue\":\"000000000000000000000000000000000000000000000000000000000000001b\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1}]}");
    }

    #[test]
    fn test_serialize_struct_logger_result_empty_stack_empty_memory() {
        let struct_logger_result = StructLoggerResult {
            gas: 20000,
            failed: false,
            return_value: "000000000000000000000000000000000000000000000000000000000000001b"
                .to_string(),
            struct_logs: vec![StructLog {
                pc: 0,
                op: "PUSH1",
                gas: 0,
                gas_cost: 0,
                depth: 1,
                stack: Some(vec![]),
                memory: Some(vec![]),
                return_data: None,
                storage: None,
                refund: 0,
                error: None,
            }],
        };
        assert_eq!(serde_json::to_string(&struct_logger_result).unwrap(), "{\"gas\":20000,\"failed\":false,\"returnValue\":\"000000000000000000000000000000000000000000000000000000000000001b\",\"structLogs\":[{\"pc\":0,\"op\":\"PUSH1\",\"gas\":0,\"gasCost\":0,\"depth\":1,\"stack\":[],\"memory\":[]}]}");
    }
}
