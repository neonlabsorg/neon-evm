use std::ops::Deref;

use crate::account::{Holder, Operator, OperatorBalance, OperatorBalanceValidator, Treasury};
use crate::debug::log_data;
use crate::error::Result;
use crate::gasometer::Gasometer;
use crate::platform::Solana;
use crate::types::{boxx::boxx, Transaction, TrxView};
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Execute Ethereum transaction in a single Solana transaction
pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Execute Transaction from Instruction with Solana call");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let messsage = &instruction[4..];

    let holder = accounts[0].clone();
    let holder = Holder::from_account_info(program_id, &holder)?;

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1])? };
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    holder.validate_owner(&operator)?;
    holder.init_heap(0)?;

    let trx = boxx(Transaction::from_rlp(messsage)?);
    let origin = trx.recover_caller_address()?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(trx.deref())?;
    let miner_address = operator_balance.miner(origin);

    log_data(&[b"HASH", &trx.hash()]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    let mut accounts_db = Solana::new_with_solana_call(&accounts[1..], operator, operator_balance)?;

    let mut gasometer = Gasometer::new(U256::ZERO, accounts_db.operator_account())?;
    gasometer.record_address_lookup_table(accounts);

    accounts_db.pay_to_treasury(treasury)?;
    super::transaction_execute::execute_with_solana_call(accounts_db, gasometer, trx, origin)
}
