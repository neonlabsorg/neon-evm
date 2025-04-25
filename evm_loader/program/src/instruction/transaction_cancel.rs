use std::cmp::min;

use crate::account::{AccountsDB, BalanceAccount, Operator, OperatorBalanceAccount, StateAccount};
use crate::config::{DEFAULT_CHAIN_ID, LAST_ITERATION_COST};
use crate::debug::log_data;
use crate::error::{Error, Result};

use crate::priority_gas_calculator::calc_priority_gas;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Cancel Transaction");

    let transaction_hash = array_ref![instruction, 0, 32];

    let storage_info = accounts[0].clone();
    let operator = Operator::from_account(&accounts[1])?;
    let operator_balance = OperatorBalanceAccount::from_account(program_id, &accounts[2])?;

    operator_balance.validate_owner(&operator)?;

    log_data(&[b"HASH", transaction_hash]);
    log_data(&[b"MINER", operator_balance.address().as_bytes()]);

    let accounts_db = AccountsDB::new(&accounts[3..], operator, Some(operator_balance), None, None);
    let storage = StateAccount::restore_without_revision_check(program_id, &storage_info)?;

    validate(&storage, transaction_hash)?;
    execute(program_id, accounts_db, storage)
}

fn validate(storage: &StateAccount, transaction_hash: &[u8; 32]) -> Result<()> {
    if &storage.trx().hash() != transaction_hash {
        return Err(Error::HolderInvalidHash(
            storage.trx().hash(),
            *transaction_hash,
        ));
    }

    Ok(())
}

fn execute<'a>(
    program_id: &Pubkey,
    accounts: AccountsDB<'a>,
    mut storage: StateAccount<'a>,
) -> Result<()> {
    let trx = storage.trx();
    let trx_chain_id = trx.chain_id().unwrap_or(DEFAULT_CHAIN_ID);
    let priority_gas = calc_priority_gas(trx)?;

    let used_gas = min(
        storage.gas_available(),
        U256::from(LAST_ITERATION_COST + priority_gas),
    );
    let total_used_gas = storage.gas_used() + used_gas;

    log_data(&[
        b"GAS",
        &used_gas.to_le_bytes(),
        &total_used_gas.to_le_bytes(),
    ]);

    let _ = storage.consume_gas(used_gas, accounts.try_operator_balance()); // ignore error

    let origin = storage.trx_origin();
    let (origin_pubkey, _) = origin.find_balance_address(program_id, trx_chain_id);

    // Do not refund unused gas for the scheduled transaction - it happens in the `scheduled_transaction_finish`.
    if !storage.trx().is_scheduled_tx() {
        let origin_info = accounts.get(&origin_pubkey).clone();
        let mut balance = BalanceAccount::from_account(program_id, origin_info)?;
        balance.increment_revision(&Rent::get()?, &accounts)?;

        storage.refund_unused_gas(&mut balance)?;
    }

    storage.cancel(program_id)
}
