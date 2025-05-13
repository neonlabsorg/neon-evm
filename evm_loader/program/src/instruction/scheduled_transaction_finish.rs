use crate::account::{
    Operator, OperatorBalanceAccount, OperatorBalanceValidator, StateAccount, TransactionTree,
};
use crate::config::TREE_ACCOUNT_FINISH_TRANSACTION_GAS;
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::evm::ExitStatus;
use crate::executor::ExecutorStateData;
use crate::types::TrxView;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Finalize Scheduled Transaction");

    let mut transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;
    let mut operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    let storage_account = &accounts[0];
    let storage_key = accounts[0].key;
    let header = StateAccount::recover_plain_header(program_id, storage_account)?;

    operator_balance.validate_owner(&operator)?;
    operator_balance.validate_transaction(&header)?;
    let miner_address = operator_balance.miner(header.origin);

    log_data(&[b"HASH", &header.hash()]);
    log_data(&[b"MINER", miner_address.as_bytes()]);

    {
        // Validate.
        let exit_status = validate(
            &header,
            executor_state.deref_mut(),
            &transaction_tree,
            header.tree_account.as_ref(),
            storage_key,
        )?;

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

    header.finish_scheduled_tx(program_id, storage_account);

    Ok(())
}

fn validate<'b>(
    trx: &impl TrxView,
    executor_state: &'b mut ExecutorStateData,
    tree: &TransactionTree,
    tree_account: Option<&Pubkey>,
    state_key: &Pubkey,
) -> Result<&'b ExitStatus> {
    // Validate if it's a scheduled transaction at all.
    if !trx.is_scheduled_tx() {
        return Err(Error::NotScheduledTransaction);
    }

    // Validate if the tree account is the one we used at the transaction start.
    let trx_tree_account = tree_account
        .expect("Unreachable code path: validation in the State Account contains a bug.");

    let actual_tree_pubkey = *tree.info().key;
    if *trx_tree_account != actual_tree_pubkey {
        return Err(Error::ScheduledTxInvalidTreeAccount(
            *trx_tree_account,
            actual_tree_pubkey,
        ));
    }

    let exit_status = executor_state
        .exit_status
        .as_ref()
        .ok_or(Error::ScheduledTxNoExitStatus(*state_key))?;
    Ok(&exit_status)
}
