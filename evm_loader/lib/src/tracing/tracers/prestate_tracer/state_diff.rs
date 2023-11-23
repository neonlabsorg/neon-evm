use ethnum::U256;
use std::collections::BTreeMap;

use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Account, PrestateTracerDiffResult, State};
use evm_loader::evm::Buffer;
use evm_loader::executor::ExecutorState;
use evm_loader::types::hexbytes::HexBytes;
use evm_loader::types::Address;

use crate::NeonError;

pub async fn build_states(
    backend: &mut ExecutorState<'_, impl AccountStorage>,
    from: Address,
    tx_fee: U256,
    chain_id: u64,
) -> Result<PrestateTracerDiffResult, NeonError> {
    Ok(PrestateTracerDiffResult {
        post: build_post_state(from, tx_fee, backend, chain_id).await?,
        pre: build_pre_state(backend, chain_id).await?,
    })
}

async fn build_pre_state(
    backend: &mut ExecutorState<'_, impl AccountStorage>,
    chain_id: u64,
) -> Result<State, NeonError> {
    let mut pre_state = BTreeMap::new();

    for address in backend.backend.used_addresses().into_iter() {
        pre_state.insert(
            address,
            Account {
                balance: backend
                    .backend
                    .balance(address, chain_id)
                    .await
                    .map(to_web3_u256),
                nonce: backend.backend.nonce(address, chain_id).await,
                code: map_code(backend.backend.code(address).await),
                storage: backend.initial_storage.borrow().get(&address).cloned(),
            },
        );
    }

    Ok(pre_state)
}

async fn build_post_state(
    from: Address,
    tx_fee: U256,
    backend: &mut ExecutorState<'_, impl AccountStorage>,
    chain_id: u64,
) -> Result<State, NeonError> {
    let mut post_state = BTreeMap::new();

    for address in backend.backend.used_addresses().into_iter() {
        let mut balance = backend.balance(address, chain_id).await?;

        if address == from {
            balance -= tx_fee;
        }

        post_state.insert(
            address,
            Account {
                balance: Some(to_web3_u256(balance)),
                nonce: Some(backend.nonce(address, chain_id).await?),
                code: map_code(backend.code(address).await?),
                storage: backend.final_storage.borrow().get(&address).cloned(),
            },
        );
    }

    Ok(post_state)
}

fn map_code(buffer: Buffer) -> Option<HexBytes> {
    if buffer.is_empty() {
        None
    } else {
        Some(HexBytes(buffer.to_vec()))
    }
}

fn to_web3_u256(v: U256) -> web3::types::U256 {
    web3::types::U256::from(v.to_be_bytes())
}
