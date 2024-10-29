use crate::account::{Holder, Operator, TransactionTree};
use crate::error::Result;
use crate::instruction::scheduled_transaction_start::validate_scheduled_tx;
use crate::types::Transaction;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Instruction");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;
    let message = &instruction[4..];

    let mut holder = Holder::from_account(program_id, accounts[0].clone())?;
    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;

    holder.validate_owner(&operator)?;
    holder.init_heap(0)?;

    let trx = Transaction::scheduled_from_rlp(message)?;
    let _ = validate_scheduled_tx(&trx, tree_index)?;
    holder.validate_transaction(&trx)?;

    transaction_tree.skip_transaction(&trx)?;

    Ok(())
}
