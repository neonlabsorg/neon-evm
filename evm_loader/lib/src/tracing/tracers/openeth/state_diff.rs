use crate::tracing::tracers::state_diff::StateMap;
use std::collections::BTreeMap;
use web3::types::{AccountDiff, ChangedType, Diff, StateDiff, H160, H256};

#[must_use]
pub fn into_state_diff(state_map: StateMap) -> StateDiff {
    let mut state_diff = BTreeMap::new();

    for (address, states) in state_map {
        let pre_account = states.pre;
        let post_account = states.post;

        if pre_account.is_empty() {
            state_diff.insert(
                H160::from(address.as_bytes()),
                AccountDiff {
                    balance: build_diff(None, Some(post_account.balance)),
                    nonce: build_diff(None, Some(post_account.nonce.into())),
                    code: build_diff(None, Some(post_account.code.clone())),
                    storage: storage_diff(&pre_account.storage, &post_account.storage),
                },
            );
        } else {
            state_diff.insert(
                H160::from(address.as_bytes()),
                AccountDiff {
                    balance: build_diff(Some(pre_account.balance), Some(post_account.balance)),
                    nonce: build_diff(
                        Some(pre_account.nonce.into()),
                        Some(post_account.nonce.into()),
                    ),
                    code: build_diff(
                        Some(pre_account.code.clone()),
                        Some(post_account.code.clone()),
                    ),
                    storage: storage_diff(&pre_account.storage, &post_account.storage),
                },
            );
        }
    }

    StateDiff(state_diff)
}

fn storage_diff(
    account_initial_storage: &BTreeMap<H256, H256>,
    account_final_storage: &BTreeMap<H256, H256>,
) -> BTreeMap<H256, Diff<H256>> {
    let mut storage_diff = BTreeMap::new();

    for (key, initial_value) in account_initial_storage {
        let final_value = account_final_storage.get(key).copied();

        storage_diff.insert(*key, build_diff(Some(*initial_value), final_value));
    }

    storage_diff
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
