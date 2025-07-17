use crate::account::{Holder, Operator, OperatorBalance, TransactionTree};
use crate::error::Result;
use crate::platform::Solana;
use crate::transaction_process::scheduled;
use crate::types::EncodedTransaction;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Skip Scheduled Transaction from Instruction");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;
    let message = &instruction[4..];

    let holder = Holder::from_account_info(program_id, &accounts[0])?;
    let tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    holder.validate(&operator)?;

    let solana = Solana::new(accounts, operator, operator_balance)?;

    let encoded_transaction = EncodedTransaction::from_rlp(message);
    scheduled::skip(tree_index, encoded_transaction, tree, solana)
}
