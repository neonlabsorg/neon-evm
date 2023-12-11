use enum_dispatch::enum_dispatch;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::rc::Rc;

use ethnum::U256;
use web3::types::H256;

use crate::types::Address;

use super::{Context, ExitStatus};

#[enum_dispatch]
pub trait EventListener: Debug {
    fn event(&mut self, event: Event);
}

pub type TracerType<T> = Rc<RefCell<T>>;
pub type TracerTypeOpt<T> = Option<TracerType<T>>;

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

pub trait StorageTracer {
    fn read_storage(&self, address: Address, index: U256, value: [u8; 32]);
    fn write_storage(&self, address: Address, index: U256, value: [u8; 32]);
}

#[derive(Default)]
pub struct StorageStateTracer {
    initial_storage: RefCell<BTreeMap<Address, BTreeMap<H256, H256>>>,
    final_storage: RefCell<BTreeMap<Address, BTreeMap<H256, H256>>>,
}

impl StorageTracer for StorageStateTracer {
    fn read_storage(&self, address: Address, index: U256, value: [u8; 32]) {
        let mut initial_storage = self.initial_storage.borrow_mut();
        let account_initial_storage = initial_storage.entry(address).or_default();

        account_initial_storage
            .entry(H256::from(index.to_be_bytes()))
            .or_insert_with(|| H256::from(value));
    }

    fn write_storage(&self, address: Address, index: U256, value: [u8; 32]) {
        self.final_storage
            .borrow_mut()
            .entry(address)
            .or_default()
            .insert(H256::from(index.to_be_bytes()), H256::from(value));
    }
}

impl StorageStateTracer {
    pub fn initial_storage_for_address(&self, address: &Address) -> Option<BTreeMap<H256, H256>> {
        self.initial_storage.borrow().get(address).cloned()
    }

    pub fn final_storage_for_address(&self, address: &Address) -> Option<BTreeMap<H256, H256>> {
        self.final_storage.borrow().get(address).cloned()
    }
}
