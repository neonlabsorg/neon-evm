use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::program::System;
use crate::account::{AccountsDB, BalanceAccount, Operator, OperatorBalanceAccount};
use crate::error::Result;

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Withdraw Operator Balance Account");

    let system = System::from_account(&accounts[0])?;
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;
    let mut operator_balance = OperatorBalanceAccount::from_account(program_id, &accounts[2])?;
    let mut target_balance = BalanceAccount::from_account(program_id, accounts[3].clone())?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.withdraw(&mut target_balance)?;

    let accounts_db = AccountsDB::new(&[], operator, Some(operator_balance), Some(system), None);
    target_balance.increment_revision(&Rent::get()?, &accounts_db)
}
