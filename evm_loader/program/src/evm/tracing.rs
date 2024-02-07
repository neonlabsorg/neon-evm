use std::collections::BTreeMap;
use std::fmt::Debug;

use crate::evm::database::Database;
use ethnum::U256;
use maybe_async::maybe_async;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use web3::types::{Bytes, H256};

use crate::types::Address;

use super::{Context, ExitStatus, Reason};

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type State = BTreeMap<Address, Account>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub balance: Option<web3::types::U256>,
    pub code: Option<Bytes>,
    pub nonce: Option<u64>,
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct States {
    pub post: State,
    pub pre: State,
}

#[derive(Debug, Clone)]
pub struct EmulationResult {
    pub used_gas: u64,
}

#[maybe_async(?Send)]
pub trait EventListener {
    async fn event(
        &mut self,
        executor_state: &mut impl Database,
        event: Event,
        chain_id: u64,
    ) -> crate::error::Result<()>;
    fn into_traces(self, emulation_result: EmulationResult) -> Value;
}

/// Trace event
pub enum Event {
    BeginVM {
        context: Context,
        code: Vec<u8>,
        reason: Reason,
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
    StorageGet {
        address: Address,
        index: U256,
        value: [u8; 32],
    },
    StorageSet {
        address: Address,
        index: U256,
        value: [u8; 32],
    },
}
