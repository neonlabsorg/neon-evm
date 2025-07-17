use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

use crate::account::{BalanceAccount, StateAccount, TransactionTree};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::platform::{Platform, Solana};
use crate::types::{EncodedTransaction, Transaction};

pub fn validate_index(tx: &Transaction, instruction_index: u16) -> Result<()> {
    let Some(trx) = tx.if_scheduled() else {
        unreachable!();
    };

    if trx.index == instruction_index {
        Ok(())
    } else {
        Err(Error::ScheduledTxInvalidIndex(trx.index, instruction_index))
    }
}

pub fn skip(
    tree_index: u16,
    transaction: EncodedTransaction,
    mut tree: TransactionTree,
    mut solana: Solana,
) -> Result<()> {
    let tx = transaction.decode()?;
    if !tx.is_scheduled_tx() {
        return Err(Error::NotScheduledTransaction);
    }

    validate_index(&tx, tree_index)?;
    tree.skip_transaction(&tx)?;

    log_data(&[b"HASH", &tx.hash]);
    solana.log_miner_address(tree.payer());

    solana.use_gasometer(|g| g.record_solana_transaction_cost(tx.gas_limit()))?;
    solana.reward_operator_from_tree(&mut tree, tx.hash)
}

pub fn start<'a>(
    tree_index: u16,
    transaction: EncodedTransaction,
    holder: AccountInfo<'a>,
    holder_owner: Pubkey,
    mut solana: Solana<'a>,
    mut tree: TransactionTree<'a>,
) -> Result<()> {
    let (mut state, tx) =
        StateAccount::new_with_tree(holder, holder_owner, transaction, &mut solana, &mut tree)?;

    validate_index(&tx, tree_index)?;
    tree.start_transaction(&tx)?;

    let tx_hash = state.transaction_hash();
    let mut root = state.root_mut();

    solana.log_miner_address(root.origin());
    log_data(&[b"HASH", tx_hash.as_ref()]);

    let mut origin: BalanceAccount = solana.get_origin(&*root)?;
    if origin.nonce() == tx.nonce() {
        // Increment origin's nonce only once for the whole execution tree.
        // All transactions in the tree should have the same nonce.
        origin.increment_nonce()?;
    }

    solana.use_gasometer(|g| g.record_solana_transaction_cost(tx.gas_limit()))?;
    solana.reward_operator_from_holder(&mut root)?;

    Ok(())
}
