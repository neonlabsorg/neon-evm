use crate::account::{Holder, Operator, OperatorBalance, TransactionTree};
use crate::error::Result;
use crate::platform::Solana;
use crate::transaction_process::scheduled;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Account");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;

    let holder = Holder::from_account_info(program_id, &accounts[0])?;
    let tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    holder.validate(&operator)?;
    let rlp = holder.transaction()?;

    let mut solana = Solana::new(&accounts[1..], operator, operator_balance)?;
    solana.use_gasometer(|g| g.record_write_to_holder(&rlp));

    scheduled::skip(tree_index, rlp, tree, solana)
}
