use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::{BalanceAccount, Operator, OperatorBalanceAccount};
use crate::error::Result;

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Withdraw Operator Balance Account");

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0]) }?;
    let mut operator_balance = OperatorBalanceAccount::from_account(program_id, &accounts[1])?;
    let mut target_balance = BalanceAccount::from_account(program_id, accounts[2].clone())?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.withdraw(&mut target_balance)
}
