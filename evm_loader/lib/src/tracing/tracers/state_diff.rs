use arrayref::array_ref;
use async_trait::async_trait;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use ethnum::U256;
use web3::types::{Bytes, H256};

use crate::types::TxParams;
use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::{Event, EventListener};
use evm_loader::evm::{opcode_table, Buffer};
use evm_loader::types::Address;
use serde::{Deserialize, Serialize};

pub type StateMap = BTreeMap<Address, States>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub balance: web3::types::U256,
    pub code: Bytes,
    pub nonce: u64,
    pub storage: BTreeMap<H256, H256>,
}

impl Account {
    pub fn is_empty(&self) -> bool {
        self.balance.is_zero() && self.nonce == 0 && self.code.0.is_empty()
    }
}

// TODO NDEV-2451 - Add operator balance diff to pre and post state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct States {
    pub post: Account,
    pub pre: Account,
}

fn map_code(buffer: Buffer) -> Bytes {
    buffer.to_vec().into()
}

pub(crate) fn to_web3_u256(v: U256) -> web3::types::U256 {
    web3::types::U256::from(v.to_be_bytes())
}

#[derive(Default, Debug)]
pub struct StateDiffTracer {
    from: Address,
    gas_price: web3::types::U256,
    tx_fee: web3::types::U256,
    depth: usize,
    state_map: StateMap,
}

#[async_trait(?Send)]
impl EventListener for StateDiffTracer {
    /// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L136>
    async fn event(
        &mut self,
        executor_state: &impl Database,
        event: Event,
    ) -> evm_loader::error::Result<()> {
        match event {
            Event::BeginVM {
                context,
                chain_id,
                opcode,
                ..
            } => {
                self.depth += 1;

                if self.depth == 1 {
                    self.lookup_account(executor_state, chain_id, context.caller)
                        .await?;
                    self.lookup_account(executor_state, chain_id, context.contract)
                        .await?;

                    let value = to_web3_u256(context.value);

                    self.state_map
                        .entry(context.caller)
                        .or_default()
                        .pre
                        .balance += value;

                    self.state_map
                        .entry(context.contract)
                        .or_default()
                        .pre
                        .balance -= value;

                    self.state_map.entry(context.caller).or_default().pre.nonce -= 1;

                    if opcode == opcode_table::CREATE {
                        self.state_map
                            .entry(context.contract)
                            .or_default()
                            .pre
                            .nonce -= 1;
                    }
                }
            }
            Event::EndVM {
                context, chain_id, ..
            } => {
                if self.depth == 1 {
                    for (address, states) in &mut self.state_map {
                        states.post = Account {
                            balance: to_web3_u256(
                                executor_state.balance(*address, chain_id).await?,
                            ),
                            code: map_code(executor_state.code(*address).await?),
                            nonce: executor_state.nonce(*address, chain_id).await?,
                            storage: {
                                let mut new_storage = BTreeMap::new();

                                for key in states.pre.storage.keys() {
                                    new_storage.insert(
                                        *key,
                                        H256::from(
                                            executor_state
                                                .storage(
                                                    *address,
                                                    U256::from_be_bytes(key.to_fixed_bytes()),
                                                )
                                                .await?,
                                        ),
                                    );
                                }

                                new_storage
                            },
                        };
                    }

                    self.state_map
                        .entry(context.caller)
                        .or_default()
                        .post
                        .balance -= self.tx_fee;
                }

                self.depth -= 1;
            }
            Event::BeginStep {
                context,
                chain_id,
                opcode,
                stack,
                memory,
                ..
            } => {
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
                    | opcode_table::SENDALL
                        if !stack.is_empty() =>
                    {
                        let address = Address::from(*array_ref!(stack[stack.len() - 1], 12, 20));
                        self.lookup_account(executor_state, chain_id, address)
                            .await?;
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
        }
        Ok(())
    }
}

impl StateDiffTracer {
    pub fn new(tx: &TxParams) -> Self {
        StateDiffTracer {
            from: tx.from,
            gas_price: tx.gas_price.map(to_web3_u256).unwrap_or_default(),
            tx_fee: to_web3_u256(
                tx.actual_gas_used
                    .unwrap_or_default()
                    .saturating_mul(tx.gas_price.unwrap_or_default()),
            ),
            ..StateDiffTracer::default()
        }
    }

    /// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L276>
    async fn lookup_account(
        &mut self,
        executor_state: &impl Database,
        chain_id: u64,
        address: Address,
    ) -> evm_loader::error::Result<()> {
        match self.state_map.entry(address) {
            Entry::Vacant(entry) => {
                entry.insert(States {
                    post: Default::default(),
                    pre: Account {
                        balance: to_web3_u256(executor_state.balance(address, chain_id).await?),
                        code: map_code(executor_state.code(address).await?),
                        nonce: executor_state.nonce(address, chain_id).await?,
                        storage: BTreeMap::new(),
                    },
                });
            }
            Entry::Occupied(_) => {}
        };
        Ok(())
    }

    /// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L292>
    async fn lookup_storage(
        &mut self,
        executor_state: &impl Database,
        address: Address,
        index: H256,
    ) -> evm_loader::error::Result<()> {
        match self
            .state_map
            .entry(address)
            .or_default()
            .pre
            .storage
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

    pub fn into_state_map(mut self, emulator_gas_used: u64) -> StateMap {
        if self.tx_fee.is_zero() {
            self.state_map.entry(self.from).or_default().post.balance -=
                web3::types::U256::from(emulator_gas_used).saturating_mul(self.gas_price);
        }

        self.state_map
    }
}
