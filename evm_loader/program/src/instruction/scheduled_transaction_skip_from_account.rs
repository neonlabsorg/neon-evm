use crate::account::{Operator, TransactionTree};
use crate::error::Result;
use crate::instruction::instruction_internals::holder_parse_trx;
use crate::instruction::scheduled_transaction_start::validate_scheduled_tx;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Account");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;

    let holder = accounts[0].clone();
    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;

    let trx = holder_parse_trx(holder, &operator, program_id, true)?;
    let _ = validate_scheduled_tx(&trx, tree_index)?;

    transaction_tree.skip_transaction(&trx)?;

    Ok(())
}
