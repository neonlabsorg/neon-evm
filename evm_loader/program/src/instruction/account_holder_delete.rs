use crate::account::{delete, Holder, Operator};
use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Delete Holder Account");

    let holder_info = accounts[0].clone();
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;

    let holder = Holder::from_account_info(program_id, &holder_info)?;
    holder.validate(&operator)?;
    unsafe { delete(&holder_info, &operator) }
}
