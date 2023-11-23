use evm_loader::evm::tracing::{State, States};
use evm_loader::types::Address;
use std::collections::BTreeMap;
use tracing::{debug, info};
use web3::types::{AccountDiff, Bytes, ChangedType, Diff, StateDiff, H160, H256, U256};

pub trait StatesExt {
    fn into_state_diff(self) -> StateDiff;

    fn balance_diff(&self, address: &Address) -> Diff<U256>;

    fn nonce_diff(&self, address: &Address) -> Diff<U256>;

    fn code_diff(&self, address: &Address) -> Diff<Bytes>;

    fn storage_diff(&self, address: &Address) -> BTreeMap<H256, Diff<H256>>;
}

impl StatesExt for States {
    fn into_state_diff(self) -> StateDiff {
        let mut state_diff = BTreeMap::new();

        for address in self.pre.keys() {
            state_diff.insert(
                H160::from(address.as_bytes()),
                AccountDiff {
                    balance: self.balance_diff(address),
                    nonce: self.nonce_diff(address),
                    code: self.code_diff(address),
                    storage: self.storage_diff(address),
                },
            );
        }

        StateDiff(state_diff)
    }

    fn balance_diff(&self, address: &Address) -> Diff<U256> {
        let balance_before = self.pre.balance(address);
        let balance_after = self.post.balance(address);

        info!("balance_diff {address}: {balance_before:?} {balance_after:?}",);

        build_diff(balance_before, balance_after)
    }

    fn nonce_diff(&self, address: &Address) -> Diff<U256> {
        build_diff(self.pre.nonce(address), self.post.nonce(address))
    }

    fn code_diff(&self, address: &Address) -> Diff<Bytes> {
        build_diff(self.pre.code(address), self.post.code(address))
    }

    fn storage_diff(&self, address: &Address) -> BTreeMap<H256, Diff<H256>> {
        let account_initial_storage = self.pre.storage(address);
        debug!("account_initial_storage={account_initial_storage:?}");

        let account_final_storage = self.post.storage(address);
        debug!("account_final_storage={account_final_storage:?}");

        let account_storage_keys = account_initial_storage
            .iter()
            .chain(account_final_storage.iter())
            .flat_map(|map| map.keys().cloned());

        let mut storage_diff = BTreeMap::new();

        for key in account_storage_keys {
            let initial_value = storage_value(&account_initial_storage, &key);
            let final_value = storage_value(&account_final_storage, &key);

            storage_diff.insert(key, build_diff(initial_value, final_value));
        }

        storage_diff
    }
}

fn storage_value(storage: &Option<&BTreeMap<H256, H256>>, key: &H256) -> Option<H256> {
    storage.and_then(|m| m.get(key).cloned())
}

fn build_diff<T: Eq>(from: Option<T>, to: Option<T>) -> Diff<T> {
    match (from, to) {
        (None, Some(to)) => Diff::Born(to),
        (None, None) => Diff::Same,
        (Some(from), None) => Diff::Died(from),
        (Some(from), Some(to)) => {
            if from == to {
                Diff::Same
            } else {
                Diff::Changed(ChangedType { from, to })
            }
        }
    }
}

trait StateExt {
    fn balance(&self, address: &Address) -> Option<U256>;

    fn nonce(&self, address: &Address) -> Option<U256>;

    fn code(&self, address: &Address) -> Option<Bytes>;

    fn storage(&self, address: &Address) -> Option<&BTreeMap<H256, H256>>;
}

impl StateExt for State {
    fn balance(&self, address: &Address) -> Option<U256> {
        self.get(address).and_then(|v| v.balance)
    }

    fn nonce(&self, address: &Address) -> Option<U256> {
        self.get(address).and_then(|v| v.nonce).map(|v| v.into())
    }

    fn code(&self, address: &Address) -> Option<Bytes> {
        self.get(address).and_then(|v| v.code.clone())
    }

    fn storage(&self, address: &Address) -> Option<&BTreeMap<H256, H256>> {
        self.get(address).and_then(|v| v.storage.as_ref())
    }
}
