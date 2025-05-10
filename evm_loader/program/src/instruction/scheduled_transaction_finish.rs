use crate::account::{Operator, StateAccount, TransactionTree};
use crate::debug::log_data;
use crate::error::{Error, Result};
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Finalize Scheduled Transaction");

    let mut state_account = StateAccount::from_account(program_id, accounts[0].clone())?;
    let mut tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;

    let tx_hash = state_account.transaction_hash();

    let state_account_pubkey = state_account.pubkey();
    let mut root = state_account.root_mut();
    // `root` is accessible, but we can't use any vectors inside of it
    // because the heap layout could change between transactions

    log_data(&[b"HASH", tx_hash.as_ref()]);
    log_data(&[b"ROOT_HASH", &tree.root_trx_hash()]);

    let Some(expected_tree_pubkey) = root.plain_data.tree_account else {
        return Err(Error::NotScheduledTransaction);
    };

    if expected_tree_pubkey != tree.pubkey() {
        let error = Error::ScheduledTxInvalidTreeAccount(expected_tree_pubkey, tree.pubkey());
        return Err(error);
    }

    let Some(exit_status) = root.plain_data.scheduled_exit_status else {
        return Err(Error::ScheduledTxNoExitStatus(state_account_pubkey));
    };
    tree.end_transaction(&tx_hash.0, exit_status, &operator)?;

    root.refund_unused_gas_to_tree(&mut tree)?;

    std::mem::drop(root);
    state_account.finalize_scheduled_tx()
}
