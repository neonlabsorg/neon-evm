use std::cmp::min;

use crate::account::{BalanceAccount, Operator, OperatorBalance, PlainStateHeader, StateAccount};
use crate::config::{DEFAULT_CHAIN_ID, LAST_ITERATION_COST};
use crate::debug::log_data;
use crate::error::{Error, Result};

use crate::platform::{Platform, Solana};
use crate::priority_gas_calculator::calc_priority_gas;
use crate::types::TrxView;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Cancel Transaction");

    let transaction_hash = array_ref![instruction, 0, 32];

    let storage_info = accounts[0].clone();
    let operator = Operator::from_account_info(&accounts[1])?;
    let operator_balance = OperatorBalance::from_account_info(program_id, &accounts[2])?;

    operator_balance.validate_owner(&operator)?;

    log_data(&[b"HASH", transaction_hash]);
    log_data(&[b"MINER", operator_balance.address().as_bytes()]);

    let accounts_db = Solana::new(&accounts[1..], operator, Some(operator_balance))?;

    let mut header = StateAccount::recover_plain_header(program_id, &storage_info)?;

    validate(&header, transaction_hash)?;
    execute(program_id, accounts_db, &mut header, &storage_info)
}

fn validate(header: &PlainStateHeader, transaction_hash: &[u8; 32]) -> Result<()> {
    if &header.hash() != transaction_hash {
        return Err(Error::HolderInvalidHash(header.hash(), *transaction_hash));
    }

    Ok(())
}

fn execute(
    program_id: Pubkey,
    mut accounts: Solana,
    header: &mut PlainStateHeader,
    storage_account: &AccountInfo,
) -> Result<()> {
    let trx_chain_id = header.chain_id().unwrap_or(DEFAULT_CHAIN_ID);
    let priority_gas = calc_priority_gas(header)?;

    let used_gas = min(
        header.gas_available(),
        U256::from(LAST_ITERATION_COST + priority_gas),
    );
    let total_used_gas = header.gas_used + used_gas;

    log_data(&[
        b"GAS",
        &used_gas.to_le_bytes(),
        &total_used_gas.to_le_bytes(),
    ]);

    let _ = header.consume_gas(used_gas, accounts.try_operator_balance()); // ignore error

    let origin = header.origin;

    // Do not refund unused gas for the scheduled transaction - it happens in the `scheduled_transaction_finish`.
    if !header.is_scheduled_tx() {
        let mut origin: BalanceAccount = accounts.create_balance(origin, trx_chain_id)?;
        header.refund_unused_gas(&mut origin)?;
    }

    header.cancel(program_id, storage_account)
}
