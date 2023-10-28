use crate::NeonError;
use ethnum::U256;
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::executor::ExecutorState;
use evm_loader::types::Address;
use std::collections::BTreeMap;
use tracing::{debug, error, info};
use web3::types::{AccountDiff, Bytes, ChangedType, Diff, StateDiff, H160, H256};

pub async fn build_state_diff<B: AccountStorage>(
    from: Address,
    tx_fee: U256,
    storage: &B,
    backend: &ExecutorState<'_, B>,
) -> Result<StateDiff, NeonError> {
    let mut state_diff = BTreeMap::new();

    for address in storage.all_addresses().iter() {
        state_diff.insert(
            H160::from(address.as_bytes()),
            AccountDiff {
                balance: build_balance_diff(from, tx_fee, storage, backend, address).await?,
                nonce: build_nonce_diff(storage, backend, address).await?,
                code: build_code_diff(storage, backend, address).await?,
                storage: build_storage_diff(backend, address),
            },
        );
    }

    Ok(StateDiff(state_diff))
}

async fn build_balance_diff(
    from: Address,
    tx_fee: U256,
    storage: &impl AccountStorage,
    backend: &impl Database,
    address: &Address,
) -> Result<Diff<web3::types::U256>, NeonError> {
    let balance_before = storage.balance(address).await;

    let mut balance_after = backend.balance(address).await?;

    if *address == from {
        balance_after -= tx_fee;
    }

    info!("balance_diff {address}: {balance_before:#x} {balance_after:#x}",);

    Ok(diff_new_u256(balance_before, balance_after))
}

async fn build_nonce_diff(
    storage: &impl AccountStorage,
    backend: &impl Database,
    address: &Address,
) -> Result<Diff<web3::types::U256>, NeonError> {
    Ok(diff_new_u256(
        storage.nonce(address).await.into(),
        backend.nonce(address).await?.into(),
    ))
}

async fn build_code_diff(
    storage: &impl AccountStorage,
    backend: &impl Database,
    address: &Address,
) -> Result<Diff<Bytes>, NeonError> {
    let initial_code = storage.code(address).await.to_vec();
    let final_code = backend.code(address).await?.to_vec();

    Ok(match (initial_code.is_empty(), final_code.is_empty()) {
        (true, false) => Diff::Born(Bytes(final_code)),
        (true, true) => Diff::Same,
        (false, true) => {
            error!("Code for address={address} initial_code={initial_code:?} cannot be deleted");
            Diff::Died(Bytes(initial_code))
        }
        (false, false) => {
            if initial_code == final_code {
                Diff::Same
            } else {
                error!("Code for address={address} initial_code={initial_code:?} final_code={final_code:?} cannot be updated");
                Diff::Changed(ChangedType {
                    from: Bytes(initial_code),
                    to: Bytes(final_code),
                })
            }
        }
    })
}

fn build_storage_diff<B: AccountStorage>(
    backend: &ExecutorState<B>,
    address: &Address,
) -> BTreeMap<H256, Diff<H256>> {
    let initial_storage = backend.initial_storage.borrow();
    debug!("initial_storage={initial_storage:?}");
    let account_initial_storage = initial_storage.get(address);

    let final_storage = backend.final_storage.borrow();
    debug!("final_storage={final_storage:?}");
    let account_final_storage = final_storage.get(address);

    let account_storage_keys = account_initial_storage
        .iter()
        .chain(account_final_storage.iter())
        .flat_map(|map| map.keys());

    let mut storage_diff = BTreeMap::new();

    for key in account_storage_keys {
        let initial_value = account_initial_storage.and_then(|m| m.get(key));
        let final_value = account_final_storage.and_then(|m| m.get(key));

        let key = H256::from(key.to_be_bytes());

        match (initial_value, final_value) {
            (None, Some(final_value)) => {
                storage_diff.insert(key, Diff::Born(H256::from(final_value)));
            }
            (Some(initial_value), Some(final_value)) => {
                storage_diff.insert(
                    key,
                    if initial_value == final_value {
                        Diff::Same
                    } else {
                        Diff::Changed(ChangedType {
                            from: H256::from(initial_value),
                            to: H256::from(final_value),
                        })
                    },
                );
            }
            (Some(initial_value), None) => {
                error!("Storage key={key}, value={initial_value:?} cannot be deleted");
                storage_diff.insert(key, Diff::Died(H256::from(initial_value)));
            }
            (None, None) => {
                error!("Storage key={key} cannot be empty");
            }
        }
    }

    storage_diff
}

fn to_web3_u256(v: U256) -> web3::types::U256 {
    web3::types::U256::from(v.to_be_bytes())
}

fn diff_new_u256(from: U256, to: U256) -> Diff<web3::types::U256> {
    let from = to_web3_u256(from);
    let to = to_web3_u256(to);

    if from == web3::types::U256::zero() {
        return Diff::Born(to);
    }

    if to == web3::types::U256::zero() {
        return Diff::Died(from);
    }

    if from == to {
        return Diff::Same;
    }

    Diff::Changed(ChangedType { from, to })
}
