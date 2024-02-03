use arrayref::array_ref;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use ethnum::U256;
use web3::types::{Bytes, H256};

use crate::account_storage::EmulatorAccountStorage;
use crate::commands::get_config::BuildConfigSimulator;
use evm_loader::account_storage::AccountStorage;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Account, Event, State, States};
use evm_loader::evm::{opcode_table, Buffer, Context};
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

#[derive(Default, Debug)]
pub struct StateDiffTracer {
    tx_fee: web3::types::U256,
    depth: usize,
    context: Option<Context>,
    pre: State,
    _post: State,
}

impl StateDiffTracer {
    pub async fn event(
        &mut self,
        executor_state: &mut impl Database,
        event: Event,
        chain_id: u64,
    ) -> evm_loader::error::Result<()> {
        match event {
            Event::BeginVM {
                context,
                code: _code,
            } => {
                if self.depth == 0 {
                    self.lookup_account(executor_state, chain_id, context.caller)
                        .await?;
                    self.lookup_account(executor_state, chain_id, context.contract)
                        .await?;

                    let value = web3::types::U256::from_big_endian(&context.value.to_be_bytes());

                    self.pre.entry(context.contract).or_default().balance = Some(
                        self.pre
                            .entry(context.contract)
                            .or_default()
                            .balance
                            .unwrap()
                            - value,
                    );

                    self.pre.entry(context.caller).or_default().balance =
                        Some(self.pre.entry(context.caller).or_default().balance.unwrap() + value);

                    self.pre.entry(context.caller).or_default().nonce =
                        Some(self.pre.entry(context.caller).or_default().nonce.unwrap() - 1);
                }

                self.depth += 1;
                self.context = Some(context);
            }
            Event::EndVM { .. } => {
                self.depth -= 1;

                if self.depth == 0 {
                    let context = self.context.as_ref().unwrap();

                    self.pre.entry(context.caller).or_default().balance = Some(
                        self.pre.entry(context.caller).or_default().balance.unwrap() - self.tx_fee,
                    );
                }
            }
            Event::BeginStep {
                opcode,
                pc: _pc,
                stack,
                memory,
            } => {
                let context = self.context.as_ref().unwrap();
                let contract = context.contract;
                match opcode {
                    opcode_table::SLOAD | opcode_table::SSTORE if !stack.is_empty() => {
                        let index = H256::from(&stack[stack.len() - 1]);
                        self.lookup_storage(executor_state, contract, index).await?;
                    }
                    opcode_table::EXTCODECOPY
                    | opcode_table::EXTCODEHASH
                    | opcode_table::EXTCODESIZE
                    | opcode_table::BALANCE
                    | opcode_table::SELFDESTRUCT
                        if !stack.is_empty() =>
                    {
                        let address = Address::from(*array_ref!(stack[stack.len() - 1], 12, 20));
                        self.lookup_account(executor_state, chain_id, address)
                            .await?;

                        // todo mark caller as deleted for selfdestruct and add unit test
                    }
                    opcode_table::DELEGATECALL
                    | opcode_table::CALL
                    | opcode_table::STATICCALL
                    | opcode_table::CALLCODE
                        if stack.len() >= 5 =>
                    {
                        let address = Address::from(*array_ref!(stack[stack.len() - 2], 12, 20));
                        self.lookup_account(executor_state, chain_id, address)
                            .await?;
                    }
                    opcode_table::CREATE => {
                        let nonce = executor_state
                            .nonce(contract, context.contract_chain_id)
                            .await?;

                        let created_address = Address::from_create(&contract, nonce);
                        self.lookup_account(executor_state, chain_id, created_address)
                            .await?;
                    }
                    opcode_table::CREATE2 if stack.len() >= 4 => {
                        let offset = U256::from_be_bytes(stack[stack.len() - 2]).as_usize();
                        let length = U256::from_be_bytes(stack[stack.len() - 3]).as_usize();
                        let salt = stack[stack.len() - 4];

                        let initialization_code = &memory[offset..offset + length];
                        let created_address =
                            Address::from_create2(&contract, &salt, initialization_code);
                        self.lookup_account(executor_state, chain_id, created_address)
                            .await?;
                    }
                    _ => {}
                }
            }
            Event::EndStep { .. } => {}
            _ => {}
        }
        Ok(())
    }

    async fn lookup_account(
        &mut self,
        executor_state: &mut impl Database,
        chain_id: u64,
        address: Address,
    ) -> evm_loader::error::Result<()> {
        match self.pre.entry(address) {
            Entry::Vacant(entry) => {
                entry.insert(Account {
                    balance: Some(web3::types::U256::from(
                        executor_state
                            .balance(address, chain_id)
                            .await?
                            .to_be_bytes(),
                    )),
                    code: Some(Bytes::from(executor_state.code(address).await?.to_vec())),
                    nonce: Some(executor_state.nonce(address, chain_id).await?),
                    storage: None,
                });
            }
            Entry::Occupied(_) => {}
        };
        Ok(())
    }

    async fn lookup_storage(
        &mut self,
        executor_state: &mut impl Database,
        address: Address,
        index: H256,
    ) -> evm_loader::error::Result<()> {
        match self
            .pre
            .entry(address)
            .or_default()
            .storage
            .get_or_insert_with(BTreeMap::new)
            .entry(index)
        {
            Entry::Vacant(entry) => {
                entry.insert(H256::from(
                    executor_state
                        .storage(address, U256::from_be_bytes(index.to_fixed_bytes()))
                        .await?,
                ));
            }
            Entry::Occupied(_) => {}
        };
        Ok(())
    }
}
