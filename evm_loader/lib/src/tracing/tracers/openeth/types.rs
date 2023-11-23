/// Types copied from <https://github.com/openethereum/openethereum/blob/main/crates/rpc/src/v1/types/trace.rs>
use std::fmt;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use web3::types::{Bytes, StateDiff, H160, U256};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// A diff of some chunk of memory.
pub struct TraceResults {
    /// The output of the call/create
    pub output: Bytes,
    /// The transaction trace.
    pub state_diff: Option<StateDiff>,
    /// The transaction trace.
    pub trace: Vec<Trace>,
    /// The transaction trace.
    pub vm_trace: Option<VMTrace>,
}

/// Trace
#[derive(Debug, Clone)]
pub struct Trace {
    /// Trace address
    trace_address: Vec<usize>,
    /// Subtraces
    subtraces: usize,
    /// Action
    action: Action,
    /// Result
    result: Res,
}

impl Serialize for Trace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut struc = serializer.serialize_struct("Trace", 4)?;
        match self.action {
            Action::Call(ref call) => {
                struc.serialize_field("type", "call")?;
                struc.serialize_field("action", call)?;
            }
            Action::Create(ref create) => {
                struc.serialize_field("type", "create")?;
                struc.serialize_field("action", create)?;
            }
            Action::Suicide(ref suicide) => {
                struc.serialize_field("type", "suicide")?;
                struc.serialize_field("action", suicide)?;
            }
            Action::Reward(ref reward) => {
                struc.serialize_field("type", "reward")?;
                struc.serialize_field("action", reward)?;
            }
        }

        match self.result {
            Res::Call(ref call) => struc.serialize_field("result", call)?,
            Res::Create(ref create) => struc.serialize_field("result", create)?,
            Res::FailedCall(ref error) | Res::FailedCreate(ref error) => {
                struc.serialize_field("error", &error.to_string())?;
            }
            Res::None => struc.serialize_field("result", &None as &Option<u8>)?,
        }

        struc.serialize_field("traceAddress", &self.trace_address)?;
        struc.serialize_field("subtraces", &self.subtraces)?;

        struc.end()
    }
}

/// Action
#[derive(Debug, Clone)]
pub enum Action {
    /// Call
    Call(Call),
    /// Create
    Create(Create),
    /// Suicide
    Suicide(Suicide),
    /// Reward
    Reward(Reward),
}

/// Call response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    /// Sender
    from: H160,
    /// Recipient
    to: H160,
    /// Transfered Value
    value: U256,
    /// Gas
    gas: U256,
    /// Input data
    input: Bytes,
    /// The type of the call.
    call_type: CallType,
}

/// Call type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CallType {
    /// None
    None,
    /// Call
    Call,
    /// Call code
    CallCode,
    /// Delegate call
    DelegateCall,
    /// Static call
    StaticCall,
}

/// Create response
#[derive(Debug, Clone, Serialize)]
pub struct Create {
    /// Sender
    from: H160,
    /// Value
    value: U256,
    /// Gas
    gas: U256,
    /// Initialization code
    init: Bytes,
}

/// Suicide
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suicide {
    /// Address.
    pub address: H160,
    /// Refund address.
    pub refund_address: H160,
    /// Balance.
    pub balance: U256,
}

/// Reward action
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reward {
    /// Author's address.
    pub author: H160,
    /// Reward amount.
    pub value: U256,
    /// Reward type.
    pub reward_type: RewardType,
}

/// Reward type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RewardType {
    /// Block
    Block,
    /// Uncle
    Uncle,
    /// EmptyStep (AuthorityRound)
    EmptyStep,
    /// External (attributed as part of an external protocol)
    External,
}

#[derive(Debug, Clone, Serialize)]
/// A record of a full VM trace for a CALL/CREATE.
pub struct VMTrace {
    /// The code to be executed.
    pub code: Bytes,
    /// The operations executed.
    pub ops: Vec<VMOperation>,
}

#[derive(Debug, Clone, Serialize)]
/// A record of the execution of a single VM operation.
pub struct VMOperation {
    /// The program counter.
    pub pc: usize,
    /// The gas cost for this instruction.
    pub cost: u64,
    /// Information concerning the execution of the operation.
    pub ex: Option<VMExecutedOperation>,
    /// Subordinate trace of the CALL/CREATE if applicable.
    #[serde(bound = "VMTrace: Serialize")]
    pub sub: Option<VMTrace>,
}

#[derive(Debug, Clone, Serialize)]
/// A record of an executed VM operation.
pub struct VMExecutedOperation {
    /// The total gas used.
    pub used: u64,
    /// The stack item placed, if any.
    pub push: Vec<U256>,
    /// If altered, the memory delta.
    pub mem: Option<MemoryDiff>,
    /// The altered storage value, if any.
    pub store: Option<StorageDiff>,
}

#[derive(Debug, Clone, Serialize)]
/// A diff of some chunk of memory.
pub struct MemoryDiff {
    /// Offset into memory the change begins.
    pub off: usize,
    /// The changed data.
    pub data: Bytes,
}

#[derive(Debug, Clone, Serialize)]
/// A diff of some storage value.
pub struct StorageDiff {
    /// Which key in storage is changed.
    pub key: U256,
    /// What the value has been changed to.
    pub val: U256,
}

#[derive(Debug, Clone)]
pub enum Res {
    /// Call
    Call(CallResult),
    /// Create
    Create(CreateResult),
    /// Call failure
    FailedCall(TraceError),
    /// Creation failure
    FailedCreate(TraceError),
    /// None
    None,
}

/// Call Result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    /// Gas used
    gas_used: U256,
    /// Output bytes
    output: Bytes,
}

/// Create Result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    /// Gas used
    gas_used: U256,
    /// Code
    code: Bytes,
    /// Assigned address
    address: H160,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TraceError {
    /// `OutOfGas` is returned when transaction execution runs out of gas.
    OutOfGas,
    /// `BadJumpDestination` is returned when execution tried to move
    /// to position that wasn't marked with JUMPDEST instruction
    BadJumpDestination,
    /// `BadInstructions` is returned when given instruction is not supported
    BadInstruction,
    /// `StackUnderflow` when there is not enough stack elements to execute instruction
    StackUnderflow,
    /// When execution would exceed defined Stack Limit
    OutOfStack,
    /// When there is not enough subroutine stack elements to return from
    SubStackUnderflow,
    /// When execution would exceed defined subroutine Stack Limit
    OutOfSubStack,
    /// When the code walks into a subroutine, that is not allowed
    InvalidSubEntry,
    /// When builtin contract failed on input data
    BuiltIn,
    /// Returned on evm internal error. Should never be ignored during development.
    /// Likely to cause consensus issues.
    Internal,
    /// When execution tries to modify the state in static context
    MutableCallInStaticContext,
    /// When invalid code was attempted to deploy
    InvalidCode,
    /// Wasm error
    Wasm,
    /// Contract tried to access past the return data buffer.
    OutOfBounds,
    /// Execution has been reverted with REVERT instruction.
    Reverted,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::TraceError::{
            BadInstruction, BadJumpDestination, BuiltIn, Internal, InvalidCode, InvalidSubEntry,
            MutableCallInStaticContext, OutOfBounds, OutOfGas, OutOfStack, OutOfSubStack, Reverted,
            StackUnderflow, SubStackUnderflow, Wasm,
        };
        let message = match *self {
            OutOfGas => "Out of gas",
            BadJumpDestination => "Bad jump destination",
            BadInstruction => "Bad instruction",
            StackUnderflow => "Stack underflow",
            OutOfStack => "Out of stack",
            SubStackUnderflow => "Subroutine stack underflow",
            OutOfSubStack => "Subroutine stack overflow",
            BuiltIn => "Built-in failed",
            InvalidSubEntry => "Invalid subroutine entry",
            InvalidCode => "Invalid code",
            Wasm => "Wasm runtime error",
            Internal => "Internal error",
            MutableCallInStaticContext => "Mutable Call In Static Context",
            OutOfBounds => "Out of bounds",
            Reverted => "Reverted",
        };
        message.fmt(f)
    }
}

pub type TraceOptions = Vec<String>;

#[must_use]
pub fn to_call_analytics(flags: &TraceOptions) -> CallAnalytics {
    CallAnalytics {
        transaction_tracing: flags.contains(&("trace".to_owned())),
        vm_tracing: flags.contains(&("vmTrace".to_owned())),
        state_diffing: flags.contains(&("stateDiff".to_owned())),
    }
}

/// Options concerning what analytics we run on the call.
#[derive(Eq, PartialEq, Default, Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalytics {
    /// Make a transaction trace.
    pub transaction_tracing: bool,
    /// Make a VM trace.
    pub vm_tracing: bool,
    /// Make a diff.
    pub state_diffing: bool,
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn test_vmtrace_serialize() {
        let t = VMTrace {
            code: vec![0, 1, 2, 3].into(),
            ops: vec![
                VMOperation {
                    pc: 0,
                    cost: 10,
                    ex: None,
                    sub: None,
                },
                VMOperation {
                    pc: 1,
                    cost: 11,
                    ex: Some(VMExecutedOperation {
                        used: 10,
                        push: vec![69.into()],
                        mem: None,
                        store: None,
                    }),
                    sub: Some(VMTrace {
                        code: vec![0].into(),
                        ops: vec![VMOperation {
                            pc: 0,
                            cost: 0,
                            ex: Some(VMExecutedOperation {
                                used: 10,
                                push: vec![42.into()],
                                mem: Some(MemoryDiff {
                                    off: 42,
                                    data: vec![1, 2, 3].into(),
                                }),
                                store: Some(StorageDiff {
                                    key: 69.into(),
                                    val: 42.into(),
                                }),
                            }),
                            sub: None,
                        }],
                    }),
                },
            ],
        };
        let serialized = serde_json::to_string(&t).unwrap();
        assert_eq!(
            serialized,
            r#"{"code":"0x00010203","ops":[{"pc":0,"cost":10,"ex":null,"sub":null},{"pc":1,"cost":11,"ex":{"used":10,"push":["0x45"],"mem":null,"store":null},"sub":{"code":"0x00","ops":[{"pc":0,"cost":0,"ex":{"used":10,"push":["0x2a"],"mem":{"off":42,"data":"0x010203"},"store":{"key":"0x45","val":"0x2a"}},"sub":null}]}}]}"#
        );
    }
}
