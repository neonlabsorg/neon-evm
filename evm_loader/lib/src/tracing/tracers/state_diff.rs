use arrayref::array_ref;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use ethnum::U256;
use web3::types::{Bytes, H256};

use evm_loader::evm::database::Database;
use evm_loader::evm::tracing::Event;
use evm_loader::evm::Reason::Create;
use evm_loader::evm::{opcode_table, Buffer, Context};
use evm_loader::types::Address;
use serde::{Deserialize, Serialize};

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L39>
pub type State = BTreeMap<Address, Account>;

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L41>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub balance: web3::types::U256,
    pub code: Option<Bytes>,
    pub nonce: u64,
    pub storage: Option<BTreeMap<H256, H256>>,
}

/// See <https://github.com/ethereum/go-ethereum/blob/master/eth/tracers/native/prestate.go#L255>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct States {
    pub post: State,
    pub pre: State,
}

// TODO NDEV-2451 - Add operator balance diff to pre and post state
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
    pub tx_fee: web3::types::U256,
    pub depth: usize,
    pub context: Option<Context>,
    pub states: States,
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
                reason,
            } => {
                if self.depth == 0 {
                    self.lookup_account(executor_state, chain_id, context.caller)
                        .await?;
                    self.lookup_account(executor_state, chain_id, context.contract)
                        .await?;

                    let value = to_web3_u256(context.value);

                    self.states.pre.entry(context.caller).or_default().balance =
                        self.states.pre.entry(context.caller).or_default().balance + value;

                    self.states.pre.entry(context.contract).or_default().balance =
                        self.states.pre.entry(context.contract).or_default().balance - value;

                    self.states.pre.entry(context.caller).or_default().nonce =
                        self.states.pre.entry(context.caller).or_default().nonce - 1;

                    // TODO check how Go Ethereum handles this
                    if reason == Create {
                        self.states.pre.entry(context.contract).or_default().nonce =
                            self.states.pre.entry(context.contract).or_default().nonce - 1;
                    }
                }

                self.depth += 1;
                self.context = Some(context);
            }
            Event::EndVM { .. } => {
                self.depth -= 1;

                if self.depth == 0 {
                    let context = self.context.as_ref().unwrap();

                    for (address, account) in &mut self.states.pre {
                        self.states.post.insert(
                            *address,
                            Account {
                                balance: to_web3_u256(
                                    executor_state.balance(*address, chain_id).await?,
                                ),
                                code: map_code(executor_state.code(*address).await?),
                                nonce: executor_state.nonce(*address, chain_id).await?,
                                storage: {
                                    match account.storage.as_ref() {
                                        None => None,
                                        Some(storage) => {
                                            let mut new_storage = BTreeMap::new();

                                            for key in storage.keys() {
                                                new_storage.insert(
                                                    *key,
                                                    H256::from(
                                                        executor_state
                                                            .storage(
                                                                *address,
                                                                U256::from_be_bytes(
                                                                    key.to_fixed_bytes(),
                                                                ),
                                                            )
                                                            .await?,
                                                    ),
                                                );
                                            }

                                            Some(new_storage)
                                        }
                                    }
                                },
                            },
                        );
                    }

                    self.states.post.entry(context.caller).or_default().balance =
                        self.states.post.entry(context.caller).or_default().balance - self.tx_fee;
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

                        // TODO add unit test
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

                        // TODO Add unit test
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
        match self.states.pre.entry(address) {
            Entry::Vacant(entry) => {
                entry.insert(Account {
                    balance: to_web3_u256(executor_state.balance(address, chain_id).await?),
                    code: map_code(executor_state.code(address).await?),
                    nonce: executor_state.nonce(address, chain_id).await?,
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
            .states
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
