use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::rc::Rc;

use ethnum::U256;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use web3::types::{Bytes, H256};

use crate::types::Address;

use super::{Context, ExitStatus};

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
    pub states: States,
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

#[derive(Default)]
pub struct StorageStateTracer {
    initial_storage: RefCell<BTreeMap<Address, BTreeMap<H256, H256>>>,
    final_storage: RefCell<BTreeMap<Address, BTreeMap<H256, H256>>>,
}

impl StorageStateTracer {
    pub fn read_storage(&self, address: Address, index: U256, value: [u8; 32]) {
        let mut initial_storage = self.initial_storage.borrow_mut();
        let account_initial_storage = initial_storage.entry(address).or_default();

        account_initial_storage
            .entry(H256::from(index.to_be_bytes()))
            .or_insert_with(|| H256::from(value));
    }

    pub fn write_storage(&self, address: Address, index: U256, value: [u8; 32]) {
        self.final_storage
            .borrow_mut()
            .entry(address)
            .or_default()
            .insert(H256::from(index.to_be_bytes()), H256::from(value));
    }

    pub fn initial_storage_for_address(&self, address: &Address) -> Option<BTreeMap<H256, H256>> {
        self.initial_storage.borrow().get(address).cloned()
    }

    pub fn final_storage_for_address(&self, address: &Address) -> Option<BTreeMap<H256, H256>> {
        self.final_storage.borrow().get(address).cloned()
    }
}
