// TODO: remove after refactoring transaction processing
// This file exists to provide temporary functionality for legacy code

use std::collections::HashMap;

use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::account_info::AccountInfo;
use solana_program::clock::Clock;
use solana_program::instruction::Instruction;

use crate::account::{AllocateResult, Operator, OperatorBalance};
use crate::config::STATIC_STORAGE_LIMIT;
use crate::error::{Error, Result};
use crate::executor::Action;
use crate::{
    account::ContractAccount,
    types::{vector::Vector, Address},
};

use super::{InvokeMode, Platform, Solana};

#[maybe_async(?Send)]
impl<'a> Solana<'a> {
    #[must_use]
    pub fn operator_account(&self) -> &Operator<'a> {
        &self.operator
    }

    #[must_use]
    pub fn operator_balance(&self) -> OperatorBalance<'a> {
        if let Some(operator_balance) = &self.operator_balance {
            return operator_balance.clone();
        }

        panic_with_error!(Error::OperatorBalanceMissing);
    }

    #[must_use]
    pub fn try_operator_balance(&self) -> Option<OperatorBalance<'a>> {
        self.operator_balance.clone()
    }

    #[must_use]
    pub fn operator_info(&self) -> &AccountInfo<'a> {
        &self.operator
    }

    pub fn update_timestamped_contracts<'r>(
        &mut self,
        contracts: impl Iterator<Item = &'r Address>,
    ) -> Result<()> {
        for address in contracts {
            let clock: Clock = self.get_sysvar()?;

            let mut contract = self.get_contract(*address)?.unwrap();
            contract.update_timestamp_used_at(&clock)?;
        }

        Ok(())
    }

    pub fn transfer_gas_payment(
        &mut self,
        origin: Address,
        chain_id: u64,
        value: U256,
    ) -> Result<()> {
        if value == U256::ZERO {
            return Ok(());
        }

        let mut source = self.create_balance(origin, chain_id)?;
        let mut target = self.operator_balance();
        target.consume_gas(&mut source, value)
    }

    pub async fn allocate(&mut self, actions: &[Action]) -> Result<AllocateResult> {
        let mut total_result = AllocateResult::Ready;

        for action in actions {
            if let Action::EvmSetCode { address, code, .. } = action {
                let result = self.allocate_contract(*address, code).await?;

                if result == AllocateResult::NeedMore {
                    total_result = AllocateResult::NeedMore;
                }
            }
        }

        Ok(total_result)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn apply_state_change(&mut self, actions: &[Action]) -> Result<()> {
        let mut original_balances = HashMap::with_capacity(16);
        let mut storage = HashMap::with_capacity(16);

        for action in actions.iter() {
            match action {
                Action::EvmSetTransientStorage { .. } => {}
                Action::Transfer {
                    source,
                    target,
                    chain_id,
                    value,
                } => {
                    let mut source_acc = self.create_balance(*source, *chain_id).await?;
                    let mut target_acc = self.create_balance(*target, *chain_id).await?;

                    original_balances
                        .entry((*source, *chain_id))
                        .or_insert_with(|| source_acc.balance());
                    original_balances
                        .entry((*target, *chain_id))
                        .or_insert_with(|| target_acc.balance());

                    // SAFETY: We will increment the revisions only for updated balances later
                    unsafe { source_acc.transfer_without_revision(&mut target_acc, *value) }?;
                }
                Action::Burn {
                    source,
                    chain_id,
                    value,
                } => {
                    let mut account = self.create_balance(*source, *chain_id).await?;
                    original_balances
                        .entry((*source, *chain_id))
                        .or_insert_with(|| account.balance());

                    // SAFETY: We will increment the revisions only for updated balances later
                    unsafe { account.burn_without_revision(*value) }?;
                }
                Action::EvmSetStorage {
                    address,
                    index,
                    value,
                } => {
                    storage
                        .entry(*address)
                        .or_insert_with(|| HashMap::with_capacity(64))
                        .insert(*index, *value);
                }
                Action::EvmIncrementNonce { address, chain_id } => {
                    let mut account = self.create_balance(*address, *chain_id).await?;
                    account.increment_nonce()?;
                }
                Action::EvmSetCode {
                    address,
                    chain_id,
                    code,
                } => {
                    self.initialize_allocated_contract(*address, *chain_id, code)
                        .await?;
                }
                Action::ExternalInstruction(v) => {
                    let seeds = v
                        .seeds
                        .iter()
                        .map(|s| s.iter().map(Vector::as_slice).collect::<Vec<_>>())
                        .collect::<Vec<_>>();
                    let seeds = seeds.iter().map(Vec::as_slice).collect::<Vec<_>>();

                    let instruction = Instruction {
                        program_id: v.program_id,
                        accounts: v.accounts.to_vec(),
                        data: v.data.to_vec(),
                    };
                    self.invoke(instruction, &seeds, InvokeMode::Queued).await?;
                }
            }
        }

        // Increment balance revisions
        for ((address, chain_id), balance) in original_balances {
            let mut account = self.get_balance(address, chain_id).await?.unwrap();
            if account.balance() != balance {
                account.increment_revision()?;
            }
        }

        // Update storage accounts
        for (address, values) in storage {
            let mut contract: Option<ContractAccount> = None;
            let mut infinite_values = HashMap::with_capacity(values.len());

            for (index, value) in values {
                if index < STATIC_STORAGE_LIMIT {
                    if contract.is_none() {
                        contract = self.get_contract(address).await?;
                    }
                    let contract = contract.as_mut().unwrap();

                    let index: usize = index.as_usize();
                    contract.set_storage_value(index, &value)?;
                } else {
                    let subindex = (index & 0xFF).as_u8();
                    let index = index & !U256::new(0xFF);

                    infinite_values
                        .entry(index)
                        .or_insert_with(|| HashMap::with_capacity(32))
                        .insert(subindex, value);
                }
            }

            // Process infinite storage
            for (index, values) in infinite_values {
                let all_values_zero = values.iter().all(|v| v.1 == &[0_u8; 32]);

                let mut storage = if all_values_zero {
                    // If all values are zero, we can skip creating a storage account
                    if let Some(storage) = self.get_storage(address, index).await? {
                        storage
                    } else {
                        continue;
                    }
                } else {
                    self.create_storage(address, index).await?
                };

                for (subindex, value) in values {
                    storage.update(subindex, &value)?;
                }
            }
        }

        Ok(())
    }
}
