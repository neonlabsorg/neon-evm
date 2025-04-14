use crate::account::{delete, BorrowedAccountInfo, Holder, Operator};
use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Delete Holder Account");

    let holder_info = accounts[0].clone();
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;

    {
        let mut data = holder_info.try_borrow_mut_data()?;
        let holder = Holder::from_account(program_id, BorrowedAccountInfo::new(&holder_info, &mut data))?;
        holder.validate_owner(&operator)?;
    }
    unsafe {
        delete(&holder_info, &operator)
    }

    Ok(())
}
