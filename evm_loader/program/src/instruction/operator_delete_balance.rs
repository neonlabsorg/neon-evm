use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::{Operator, OperatorBalanceAccount};
use crate::error::Result;

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Delete Operator Balance Account");

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0]) }?;
    let operator_balance = OperatorBalanceAccount::from_account(program_id, &accounts[1])?;

    operator_balance.validate_owner(&operator)?;
    unsafe {
        operator_balance.suicide(&operator);
    }

    Ok(())
}
