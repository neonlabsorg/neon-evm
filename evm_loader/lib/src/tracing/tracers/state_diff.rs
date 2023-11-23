use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use ethnum::U256;
use web3::types::Bytes;

use crate::account_storage::EmulatorAccountStorage;
use crate::commands::get_config::BuildConfigSimulator;
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Account, State, States};
use evm_loader::evm::Buffer;
use evm_loader::executor::ExecutorState;
use evm_loader::types::Address;

use crate::rpc::Rpc;
use crate::NeonError;

// TODO NDEV-2451 - Add operator balance diff to pre and post state
#[async_trait(?Send)]
pub trait ExecutorStateExt {
    fn used_addresses(&self) -> BTreeSet<Address>;

    async fn build_states(
        &mut self,
        from: Address,
        tx_fee: U256,
        chain_id: u64,
    ) -> Result<States, NeonError>;

    async fn build_pre_state(&mut self, chain_id: u64) -> Result<State, NeonError>;

    async fn build_post_state(
        &mut self,
        from: Address,
        tx_fee: U256,
        chain_id: u64,
    ) -> Result<State, NeonError>;
}

#[async_trait(?Send)]
impl<T: Rpc + BuildConfigSimulator> ExecutorStateExt
    for ExecutorState<'_, EmulatorAccountStorage<'_, T>>
{
    fn used_addresses(&self) -> BTreeSet<Address> {
        self.backend.used_addresses.borrow().clone()
    }

    async fn build_states(
        &mut self,
        from: Address,
        tx_fee: U256,
        chain_id: u64,
    ) -> Result<States, NeonError> {
        Ok(States {
            post: self.build_post_state(from, tx_fee, chain_id).await?,
            pre: self.build_pre_state(chain_id).await?,
        })
    }

    async fn build_pre_state(&mut self, chain_id: u64) -> Result<State, NeonError> {
        let mut pre_state = BTreeMap::new();

        for address in self.used_addresses().into_iter() {
            pre_state.insert(
                address,
                Account {
                    balance: self
                        .backend
                        .balance(address, chain_id)
                        .await
                        .map(to_web3_u256),
                    nonce: self.backend.nonce(address, chain_id).await,
                    code: map_code(self.backend.code(address).await),
                    storage: self
                        .storage_state_tracer
                        .initial_storage_for_address(&address),
                },
            );
        }

        Ok(pre_state)
    }

    async fn build_post_state(
        &mut self,
        from: Address,
        tx_fee: U256,
        chain_id: u64,
    ) -> Result<State, NeonError> {
        let mut post_state = BTreeMap::new();

        for address in self.used_addresses().into_iter() {
            let mut balance = self.balance(address, chain_id).await?;

            if address == from {
                balance -= tx_fee;
            }

            post_state.insert(
                address,
                Account {
                    balance: Some(to_web3_u256(balance)),
                    nonce: Some(self.nonce(address, chain_id).await?),
                    code: map_code(self.code(address).await?),
                    storage: self
                        .storage_state_tracer
                        .final_storage_for_address(&address),
                },
            );
        }

        Ok(post_state)
    }
}

fn map_code(buffer: Buffer) -> Option<Bytes> {
    if buffer.is_empty() {
        None
    } else {
        Some(buffer.to_vec().into())
    }
}

fn to_web3_u256(v: U256) -> web3::types::U256 {
    web3::types::U256::from(v.to_be_bytes())
}
