use crate::account::{
    Operator, OperatorBalance, OperatorBalanceValidator, StateAccount, TransactionTree,
};
use crate::config::TREE_ACCOUNT_FINISH_TRANSACTION_GAS;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::types::TrxView;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Finalize Scheduled Transaction");

    let mut transaction_tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let mut operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    let storage_account = &accounts[0];
    let storage_key = accounts[0].key;
    let mut header = StateAccount::recover_plain_header(program_id, storage_account)?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&header)?;
    let miner_address = operator_balance.miner(header.origin);

    log_data(&[b"HASH", &header.hash()]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    {
        // Validate.
        validate(&header, &transaction_tree, header.tree_account.as_ref())?;

        let exit_status = header
            .tx_exit_status
            .as_ref()
            .ok_or(Error::ScheduledTxNoExitStatus(*storage_key))?
            .clone();

        // Handle gas, transaction costs to operator, refund into tree account.
        const GAS: U256 = U256::new(TREE_ACCOUNT_FINISH_TRANSACTION_GAS as u128);
        if let Some(operator_balance) = &mut operator_balance {
            // don't burn tokens in tree, because it was already reserved at the start
            operator_balance.mint(GAS)?;
        }

        let refund = header.materialize_unused_gas()?;
        transaction_tree.mint(refund)?;

        // Finalize.
        transaction_tree.end_transaction(header.hash(), exit_status)?;
    };

    header.finish_scheduled_tx(program_id, storage_account)?;

    Ok(())
}

fn validate<'b>(
    trx: &impl TrxView,
    tree: &TransactionTree,
    tree_account: Option<&Pubkey>,
) -> Result<()> {
    // Validate if it's a scheduled transaction at all.
    if !trx.is_scheduled_tx() {
        return Err(Error::NotScheduledTransaction);
    }

    // Validate if the tree account is the one we used at the transaction start.
    let trx_tree_account = tree_account
        .expect("Unreachable code path: validation in the State Account contains a bug.");

    if *trx_tree_account != tree.pubkey() {
        return Err(Error::ScheduledTxInvalidTreeAccount(
            *trx_tree_account,
            tree.pubkey(),
        ));
    }

    Ok(())
}
