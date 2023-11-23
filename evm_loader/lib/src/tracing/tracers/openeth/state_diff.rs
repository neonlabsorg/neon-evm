use evm_loader::evm::tracing::{PrestateTracerDiffResult, State};
use evm_loader::types::Address;
use std::collections::BTreeMap;
use tracing::{debug, info};
use web3::types::{AccountDiff, Bytes, ChangedType, Diff, StateDiff, H160, H256};

pub fn build_state_diff(states: PrestateTracerDiffResult) -> StateDiff {
    let mut state_diff = BTreeMap::new();

    for address in states.pre.keys() {
        state_diff.insert(
            H160::from(address.as_bytes()),
            AccountDiff {
                balance: build_balance_diff(&states, address),
                nonce: build_nonce_diff(&states, address),
                code: build_code_diff(&states, address),
                storage: build_storage_diff(&states, address),
            },
        );
    }

    StateDiff(state_diff)
}

fn build_balance_diff(
    states: &PrestateTracerDiffResult,
    address: &Address,
) -> Diff<web3::types::U256> {
    let balance_before = balance(&states.pre, address);
    let balance_after = balance(&states.post, address);

    info!("balance_diff {address}: {balance_before:?} {balance_after:?}",);

    build_diff(balance_before, balance_after)
}

fn balance(state: &State, address: &Address) -> Option<web3::types::U256> {
    state.get(address).and_then(|v| v.balance)
}

fn build_nonce_diff(
    states: &PrestateTracerDiffResult,
    address: &Address,
) -> Diff<web3::types::U256> {
    build_diff(nonce(&states.pre, address), nonce(&states.post, address))
}

fn nonce(state: &State, address: &Address) -> Option<web3::types::U256> {
    state.get(address).and_then(|v| v.nonce).map(|v| v.into())
}

fn build_code_diff(states: &PrestateTracerDiffResult, address: &Address) -> Diff<Bytes> {
    let initial_code = code(&states.pre, address);
    let final_code = code(&states.post, address);

    build_diff(initial_code, final_code)
}

fn code(state: &State, address: &Address) -> Option<Bytes> {
    state
        .get(address)
        .and_then(|v| v.code.as_ref())
        .map(|v| Bytes(v.to_vec()))
}

fn build_storage_diff(
    states: &PrestateTracerDiffResult,
    address: &Address,
) -> BTreeMap<H256, Diff<H256>> {
    let account_initial_storage = storage(&states.pre, address);
    debug!("account_initial_storage={account_initial_storage:?}");

    let account_final_storage = storage(&states.post, address);
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

fn storage<'a>(state: &'a State, address: &Address) -> Option<&'a BTreeMap<H256, H256>> {
    state.get(address).and_then(|v| v.storage.as_ref())
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
