use std::collections::BTreeMap;

use evm_loader::evm::tracing::{State, States};
use serde::{Deserialize, Serialize};
use web3::types::{Bytes, H256, U256};

use evm_loader::types::Address;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type PrestateTracerPreState = BTreeMap<Address, PrestateTracerPreAccount>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type PrestateTracerPostState = BTreeMap<Address, PrestateTracerPostAccount>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerPreAccount {
    pub balance: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Bytes>,
    pub nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerPostAccount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<U256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Bytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrestateTracerDiffModeResult {
    pub post: PrestateTracerPostState,
    pub pre: PrestateTracerPreState,
}

pub fn build_prestate_tracer_pre_state(pre: State) -> PrestateTracerPreState {
    let mut result = BTreeMap::new();

    for (address, account) in pre {
        result.insert(
            address,
            PrestateTracerPreAccount {
                balance: account.balance.unwrap_or_default(),
                code: account.code,
                nonce: account.nonce.unwrap_or_default(),
                storage: account.storage,
            },
        );
    }

    result
}

pub fn build_prestate_tracer_diff_mode_result(states: States) -> PrestateTracerDiffModeResult {
    let pre = build_prestate_tracer_pre_state(states.pre);

    let mut post = BTreeMap::new();

    for (address, pre_account) in pre.iter() {
        let post_account = states.post.get(address);

        post.insert(
            *address,
            PrestateTracerPostAccount {
                balance: post_account.and_then(|a| a.balance).and_then(|balance| {
                    if balance != pre_account.balance {
                        Some(balance)
                    } else {
                        None
                    }
                }),
                code: post_account.and_then(|account| {
                    if account.code != pre_account.code {
                        account.code.clone()
                    } else {
                        None
                    }
                }),
                nonce: post_account.and_then(|a| a.nonce).and_then(|nonce| {
                    if nonce != pre_account.nonce {
                        Some(nonce)
                    } else {
                        None
                    }
                }),
                storage: post_account
                    .and_then(|a| a.storage.as_ref())
                    .and_then(|final_storage| {
                        pre_account.storage.as_ref().map(|initial_storage| {
                            build_storage_diff(initial_storage, final_storage)
                        })
                    }),
            },
        );
    }

    PrestateTracerDiffModeResult { post, pre }
}

fn build_storage_diff(
    account_initial_storage: &BTreeMap<H256, H256>,
    account_final_storage: &BTreeMap<H256, H256>,
) -> BTreeMap<H256, H256> {
    let account_storage_keys = account_initial_storage
        .keys()
        .chain(account_final_storage.keys())
        .cloned();

    let mut storage_diff = BTreeMap::new();

    for key in account_storage_keys {
        let initial_value = account_initial_storage.get(&key).cloned();
        let final_value = account_final_storage.get(&key).cloned();

        match (initial_value, final_value) {
            (None, Some(final_value)) => {
                storage_diff.insert(key, final_value);
            }
            (Some(initial_value), Some(final_value)) if initial_value != final_value => {
                storage_diff.insert(key, final_value);
            }
            _ => {}
        }
    }

    storage_diff
}
