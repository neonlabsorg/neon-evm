use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::rc::Rc;

use crate::types::hexbytes::HexBytes;
use crate::types::Address;
use ethnum::U256;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use web3::types::H256;

use super::{Context, ExitStatus};

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type State = BTreeMap<Address, Account>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub balance: Option<web3::types::U256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<HexBytes>,
    pub nonce: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerDiffResult {
    pub post: State,
    pub pre: State,
}

#[derive(Debug, Clone)]
pub struct EmulationResult {
    pub exit_status: String,
    pub result: Vec<u8>,
    pub steps_executed: u64,
    pub used_gas: u64,
    pub states: PrestateTracerDiffResult,
}

pub trait EventListener: Debug {
    fn event(&mut self, event: Event);
    fn into_traces(self: Box<Self>, emulation_result: EmulationResult) -> Value;
}

pub type TracerType = Rc<RefCell<Box<dyn EventListener>>>;
pub type TracerTypeOpt = Option<TracerType>;

/// Trace event
pub enum Event {
    BeginVM {
        context: Context,
        code: Vec<u8>,
    },
    EndVM {
        status: ExitStatus,
    },
    BeginStep {
        opcode: u8,
        pc: usize,
        stack: Vec<[u8; 32]>,
        memory: Vec<u8>,
    },
    EndStep {
        gas_used: u64,
        return_data: Option<Vec<u8>>,
    },
    StorageAccess {
        index: U256,
        value: [u8; 32],
    },
}
