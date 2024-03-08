use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use web3::types::{Bytes, H256, U256};

use crate::tracing::tracers::state_diff::StateMap;
use evm_loader::types::Address;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
type PrestateTracerState = BTreeMap<Address, PrestateTracerAccount>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerAccount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<U256>,
    #[serde(skip_serializing_if = "is_empty")]
    pub code: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<u64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub storage: BTreeMap<H256, H256>,
}

fn is_empty(bytes: &Option<Bytes>) -> bool {
    match bytes {
        None => true,
        Some(bytes) => bytes.0.is_empty(),
    }
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerDiffModeResult {
    pub post: PrestateTracerState,
    pub pre: PrestateTracerState,
}

pub fn build_prestate_tracer_pre_state(state_map: StateMap) -> PrestateTracerState {
    let mut result = BTreeMap::new();

    for (address, states) in state_map {
        let pre_account = states.pre;

        if pre_account.is_empty() {
            continue;
        }

        result.insert(
            address,
            PrestateTracerAccount {
                balance: Some(pre_account.balance),
                code: Some(pre_account.code),
                nonce: Some(pre_account.nonce),
                storage: pre_account.storage,
            },
        );
    }

    result
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L186>
pub fn build_prestate_tracer_diff_mode_result(state_map: StateMap) -> PrestateTracerDiffModeResult {
    let mut pre = build_prestate_tracer_pre_state(state_map.clone());

    let mut post = BTreeMap::new();

    for (address, states) in state_map {
        let pre_account = states.pre;
        let post_account = states.post;

        let mut modified = false;

        let balance = if post_account.balance != pre_account.balance {
            modified = true;
            Some(post_account.balance)
        } else {
            None
        };

        let code = if post_account.code != pre_account.code {
            modified = true;
            Some(post_account.code.clone())
        } else {
            None
        };

        let nonce = if post_account.nonce != pre_account.nonce {
            modified = true;
            Some(post_account.nonce)
        } else {
            None
        };

        let mut storage = BTreeMap::new();

        for (key, initial_value) in pre_account.storage {
            // don't include the empty slot
            if initial_value == H256::zero() {
                pre.entry(address).and_modify(|account| {
                    account.storage.remove(&key);
                });
            }

            let final_value = post_account.storage.get(&key).cloned().unwrap_or_default();

            // Omit unchanged slots
            if initial_value == final_value {
                pre.entry(address).and_modify(|account| {
                    account.storage.remove(&key);
                });
            } else {
                modified = true;
                if final_value != H256::zero() {
                    storage.insert(key, final_value);
                }
            }
        }

        if modified {
            post.insert(
                address,
                PrestateTracerAccount {
                    balance,
                    code,
                    nonce,
                    storage,
                },
            );
        } else {
            // if state is not modified, then no need to include into the pre state
            pre.remove(&address);
        }
    }

    PrestateTracerDiffModeResult { post, pre }
}
