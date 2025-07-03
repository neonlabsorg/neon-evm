use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::{Operator, OperatorBalance};
use crate::error::Result;
use crate::platform::Solana;

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Withdraw Operator Balance Account");

    // #0 System program
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;
    let operator_balance = OperatorBalance::from_account_info(program_id, &accounts[2])?;
    // #3 Target balance account

    let mut solana = Solana::new(accounts, operator, Some(operator_balance))?;
    solana.withdraw_operator_balance()
}
