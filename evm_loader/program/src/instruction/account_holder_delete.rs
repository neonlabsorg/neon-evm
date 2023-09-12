use crate::account::{Holder, Operator};
use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    solana_program::msg!("Instruction: Delete Holder Account");

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;
    let holder = Holder::from_account(program_id, accounts[0].clone())?;

    holder.validate_owner(&operator)?;
    unsafe {
        holder.suicide(&operator);
    }

    Ok(())
}
