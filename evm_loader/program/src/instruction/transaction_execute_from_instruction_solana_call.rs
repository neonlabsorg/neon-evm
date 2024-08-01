use crate::account::{
    program, AccountsDB, Holder, Operator, OperatorBalanceAccount, OperatorBalanceValidator,
    Treasury,
};
use crate::debug::log_data;
use crate::error::Result;
use crate::gasometer::Gasometer;
use crate::types::{boxx::boxx, Transaction};
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Execute Ethereum transaction in a single Solana transaction
pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Execute Transaction from Instruction with Solana call");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);
    let messsage = &instruction[4..];

    let mut holder = Holder::from_account(program_id, accounts[0].clone())?;
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1])? };
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;
    let system = program::System::from_account(&accounts[4])?;

    holder.validate_owner(&operator)?;
    holder.init_heap(0)?;

    let trx = boxx(Transaction::from_rlp(messsage)?);
    let origin = trx.recover_caller_address()?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&trx)?;
    let miner_address = operator_balance.miner(origin);

    log_data(&[b"HASH", &trx.hash()]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    let accounts_db = AccountsDB::new(
        &accounts[4..],
        operator,
        operator_balance,
        Some(system),
        Some(treasury),
    );

    let mut gasometer = Gasometer::new(U256::ZERO, accounts_db.operator())?;
    gasometer.record_solana_transaction_cost();
    gasometer.record_address_lookup_table(accounts);

    super::transaction_execute::execute_with_solana_call(accounts_db, gasometer, trx, origin)
}
