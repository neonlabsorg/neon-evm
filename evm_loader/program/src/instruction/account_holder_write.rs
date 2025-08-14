use crate::account::{Holder, Operator};
use crate::debug::log_data;
use crate::error::Result;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Write To Holder");

    let transaction_hash = *array_ref![instruction, 0, 32];
    let offset = usize::from_le_bytes(*array_ref![instruction, 32, 8]);
    let data = &instruction[32 + 8..];

    let holder_info = accounts[0].clone();
    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[1]) }?;

    let mut holder = Holder::from_account_info(program_id, &holder_info)?;
    holder.validate(&operator)?;
    holder.update_transaction_hash(transaction_hash);

    log_data(&[b"HASH", &transaction_hash]);

    holder.write(offset, data)?;

    Ok(())
}
