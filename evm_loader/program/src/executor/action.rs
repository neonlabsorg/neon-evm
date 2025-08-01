use allocator_api2::alloc::Allocator;
use ethnum::U256;
use maybe_async::maybe_async;
use mpl_token_metadata::programs::MPL_TOKEN_METADATA_ID;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    rent::Rent,
};

use crate::{
    account::{AllocateResult, ContractAccount},
    config::STATIC_STORAGE_LIMIT,
    error::{Error, Result},
    platform::{InvokeMode, Platform},
    types::{
        seeds::{InvokeSeeds, SeedsRef},
        vector::{Vector, VectorMap},
        Address,
    },
};

use super::owned_account::OwnedAccountInfo;

#[repr(C, u8)]
pub enum Action<A: Allocator> {
    ExternalInstruction {
        program_id: Pubkey,
        accounts: Vector<AccountMeta, A>,
        data: Vector<u8, A>,
        seeds: InvokeSeeds<A>,
    },
    Transfer {
        source: Address,
        target: Address,
        chain_id: u64,
        value: U256,
    },
    Burn {
        source: Address,
        chain_id: u64,
        value: U256,
    },
    EvmSetStorage {
        address: Address,
        index: U256,
        value: [u8; 32],
    },
    EvmIncrementNonce {
        address: Address,
        chain_id: u64,
    },
    EvmStartCreate {
        address: Address,
        chain_id: u64,
    },
    EvmEndCreate {
        address: Address,
        code: Vector<u8, A>,
    },
}

pub struct IterativeActions<A: Allocator> {
    storage: Vector<Action<A>, A>,
    stack: Vector<usize, A>,
}

impl<A: Allocator + Copy> IterativeActions<A> {
    pub fn new_in(allocator: A) -> Self {
        Self {
            storage: Vector::with_capacity_in(32, allocator),
            stack: Vector::with_capacity_in(8, allocator),
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Action<A>> {
        self.storage.iter()
    }

    pub fn push(&mut self, action: Action<A>) {
        self.storage.push(action);
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn snapshot(&mut self) {
        self.stack.push(self.storage.len());
    }

    pub fn revert(&mut self) {
        let storage_len = self
            .stack
            .pop()
            .expect("Fatal Error: Inconsistent EVM Call Stack");

        self.storage.truncate(storage_len);
    }

    pub fn commit(&mut self) {
        self.stack
            .pop()
            .expect("Fatal Error: Inconsistent EVM Call Stack");
    }

    pub fn apply_to_balance(&self, address: Address, chain_id: u64, balance: &mut U256) {
        for action in &self.storage {
            match action {
                Action::Transfer {
                    source,
                    target,
                    chain_id: id,
                    value,
                } => {
                    if (source == &address) && (id == &chain_id) {
                        *balance = balance.checked_sub(*value).expect("Inconsistent balance");
                    }
                    if (target == &address) && (id == &chain_id) {
                        *balance = balance.checked_add(*value).expect("Inconsistent balance");
                    }
                }
                Action::Burn {
                    source,
                    chain_id: id,
                    value,
                } => {
                    if (source == &address) && (id == &chain_id) {
                        *balance = balance.checked_sub(*value).expect("Inconsistent balance");
                    }
                }
                _ => {}
            }
        }
    }

    pub fn apply_to_nonce(&self, from_address: Address, from_chain_id: u64, nonce: &mut u64) {
        let mut increment = 0_u64;

        for action in &self.storage {
            if let Action::EvmIncrementNonce { address, chain_id } = action {
                if (&from_address == address) && (&from_chain_id == chain_id) {
                    // increment won't overflow, it's not possible to have more than 2^64 actions
                    increment += 1;
                }
            }
        }

        *nonce = nonce.checked_add(increment).expect("Inconsistent nonce");
    }

    pub fn find_code(&self, from_address: Address) -> Option<&[u8]> {
        for action in &self.storage {
            if let Action::EvmEndCreate { address, code } = action {
                if address == &from_address {
                    return Some(code);
                }
            }
        }
        None
    }

    pub fn find_contract_chain_id(&self, from_address: Address) -> Option<u64> {
        for action in &self.storage {
            if let Action::EvmStartCreate { address, chain_id } = action {
                if address == &from_address {
                    return Some(*chain_id);
                }
            }
        }
        None
    }

    pub fn find_storage(&self, from_address: Address, from_index: U256) -> Option<&[u8; 32]> {
        // Iterate in reverse order to find the most recent storage value
        for action in self.storage.iter().rev() {
            if let Action::EvmSetStorage {
                address,
                index,
                value,
            } = action
            {
                if (address == &from_address) && (index == &from_index) {
                    return Some(value);
                }
            }
        }

        None
    }

    pub fn collect_external_accounts(&self) -> Vec<&AccountMeta> {
        self.storage
            .iter()
            .filter_map(|a| {
                if let Action::ExternalInstruction { accounts, .. } = a {
                    Some(accounts)
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>()
    }

    pub fn apply_to_external_accounts(
        &self,
        rent: &Rent,
        accounts: &mut VectorMap<Pubkey, OwnedAccountInfo>,
    ) -> Result<()> {
        for action in &self.storage {
            if let Action::ExternalInstruction {
                program_id,
                data,
                accounts: meta,
                ..
            } = action
            {
                match program_id {
                    program_id if solana_program::system_program::check_id(program_id) => {
                        crate::external_programs::system::emulate(data, meta, accounts)?;
                    }
                    program_id if spl_token::check_id(program_id) => {
                        crate::external_programs::spl_token::emulate(data, meta, accounts)?;
                    }
                    program_id if spl_associated_token_account::check_id(program_id) => {
                        crate::external_programs::spl_associated_token::emulate(
                            data, meta, accounts, rent,
                        )?;
                    }
                    program_id if &MPL_TOKEN_METADATA_ID == program_id => {
                        crate::external_programs::metaplex::emulate(data, meta, accounts, rent)?;
                    }
                    _ => {
                        return Err(Error::Custom(format!(
                            "Unknown external program for emulate: {program_id}"
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

impl<A: Allocator> IntoIterator for IterativeActions<A> {
    type Item = Action<A>;
    type IntoIter = allocator_api2::vec::IntoIter<Self::Item, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.into_iter()
    }
}

impl<'r, A: Allocator> IntoIterator for &'r IterativeActions<A> {
    type Item = &'r Action<A>;
    type IntoIter = std::slice::Iter<'r, Action<A>>;

    fn into_iter(self) -> Self::IntoIter {
        self.storage.iter()
    }
}

#[maybe_async(?Send)]
pub trait ActionExecutor {
    async fn allocate<'a>(&self, platform: &mut impl Platform<'a>) -> Result<AllocateResult>;
    async fn execute<'a>(&mut self, platform: &mut impl Platform<'a>) -> Result<()>;
}

#[maybe_async(?Send)]
impl<A: Allocator> ActionExecutor for IterativeActions<A> {
    async fn allocate<'a>(&self, platform: &mut impl Platform<'a>) -> Result<AllocateResult> {
        let mut total_result = AllocateResult::Ready;

        for action in &self.storage {
            if let Action::EvmEndCreate { address, code } = action {
                let result = platform.allocate_contract(*address, code).await?;

                if result == AllocateResult::NeedMore {
                    total_result = AllocateResult::NeedMore;
                }
            }
        }

        Ok(total_result)
    }

    #[allow(clippy::too_many_lines)]
    async fn execute<'a>(&mut self, platform: &mut impl Platform<'a>) -> Result<()> {
        let mut original_balances = VectorMap::with_capacity(16);
        let mut storage = VectorMap::with_capacity(16);
        let mut contracts = VectorMap::with_capacity(8);

        for action in self.storage.drain(..) {
            match action {
                Action::Transfer {
                    source,
                    target,
                    chain_id,
                    value,
                } => {
                    let mut source_acc = platform.create_balance(source, chain_id).await?;
                    let mut target_acc = platform.create_balance(target, chain_id).await?;

                    original_balances
                        .entry((source, chain_id))
                        .or_insert_with(|| source_acc.balance());
                    original_balances
                        .entry((target, chain_id))
                        .or_insert_with(|| target_acc.balance());

                    // SAFETY: We will increment the revisions only for updated balances later
                    unsafe { source_acc.transfer_without_revision(&mut target_acc, value) }?;
                }
                Action::Burn {
                    source,
                    chain_id,
                    value,
                } => {
                    let mut account = platform.create_balance(source, chain_id).await?;
                    original_balances
                        .entry((source, chain_id))
                        .or_insert_with(|| account.balance());

                    // SAFETY: We will increment the revisions only for updated balances later
                    unsafe { account.burn_without_revision(value) }?;
                }
                Action::EvmSetStorage {
                    address,
                    index,
                    value,
                } => {
                    storage
                        .entry(address)
                        .or_insert_with(|| VectorMap::with_capacity(64))
                        .insert(index, value);
                }
                Action::EvmIncrementNonce { address, chain_id } => {
                    let mut account = platform.create_balance(address, chain_id).await?;
                    account.increment_nonce()?;
                }
                Action::EvmStartCreate { address, chain_id } => {
                    contracts.insert(address, (chain_id, None));
                }
                Action::EvmEndCreate { address, code } => {
                    let Some(contract) = contracts.get_mut(&address) else {
                        unreachable!();
                    };
                    contract.1 = Some(code);
                }
                Action::ExternalInstruction {
                    program_id,
                    accounts,
                    data,
                    seeds,
                    ..
                } => {
                    let seeds: Vec<SeedsRef> = seeds.data.iter().map(SeedsRef::new).collect();
                    let seeds: Vec<&[&[u8]]> = seeds.iter().map(SeedsRef::as_slices).collect();

                    let instruction = Instruction {
                        program_id,
                        accounts: accounts.to_vec(),
                        data: data.to_vec(),
                    };
                    platform
                        .invoke(instruction, &seeds, InvokeMode::Queued)
                        .await?;
                }
            }
        }

        // Increment balance revisions
        for ((address, chain_id), balance) in original_balances {
            let mut account = platform.get_balance(address, chain_id).await?.unwrap();
            if account.balance() != balance {
                account.increment_revision()?;
            }
        }

        // Initialize contract accounts
        for (address, (chain_id, code)) in contracts {
            let code = code.unwrap();
            platform
                .initialize_allocated_contract(address, chain_id, &code)
                .await?;
        }

        // Update storage accounts
        for (address, values) in storage {
            let mut contract: Option<ContractAccount> = None;
            let mut infinite_values = VectorMap::with_capacity(values.len());

            for (index, value) in values {
                if index < STATIC_STORAGE_LIMIT {
                    if contract.is_none() {
                        contract = platform.get_contract(address).await?;
                    }
                    let contract = contract.as_mut().unwrap();

                    let index: usize = index.as_usize();
                    contract.set_storage_value(index, &value)?;
                } else {
                    let subindex = (index & 0xFF).as_u8();
                    let index = index & !U256::new(0xFF);

                    infinite_values
                        .entry(index)
                        .or_insert_with(|| VectorMap::with_capacity(32))
                        .insert(subindex, value);
                }
            }

            // Process infinite storage
            for (index, values) in infinite_values {
                let all_values_zero = values.iter().all(|v| v.1 == [0_u8; 32]);

                let mut storage = if all_values_zero {
                    // If all values are zero, we can skip creating a storage account
                    if let Some(storage) = platform.get_storage(address, index).await? {
                        storage
                    } else {
                        continue;
                    }
                } else {
                    platform.create_storage(address, index).await?
                };

                for (subindex, value) in values {
                    storage.update(subindex, &value)?;
                }
            }
        }

        Ok(())
    }
}
