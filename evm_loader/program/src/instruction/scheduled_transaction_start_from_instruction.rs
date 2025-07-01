use crate::account::{
    program, AccountDispatch, AccountsDB, Holder, Operator, OperatorBalance,
    OperatorBalanceValidator, StateAccount, TransactionTree, TAG_HOLDER,
    TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::scheduled_transaction_start::{do_scheduled_start, validate_scheduled_tx};
use crate::types::Transaction;
use arrayref::array_ref;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Start Scheduled Transaction from Instruction");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;
    let message = &instruction[4..];

    let holder = accounts[0].clone();
    let transaction_tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;
    let system = program::System::from_account_info(&accounts[4])?;

    operator_balance.validate_owner(&operator)?;

    let accounts_db = AccountsDB::new(
        &accounts[5..],
        operator.clone(),
        operator_balance.clone(),
        Some(system),
        None,
    );

    match holder.tag(program_id)? {
        TAG_HOLDER | TAG_STATE_FINALIZED => {
            // TODO clarify how it works with STATE_FINALIZED.
            Holder::init_holder_heap(program_id, &holder, 0)?;
            let trx = Transaction::scheduled_from_rlp(message)?;

            let scheduled_trx = validate_scheduled_tx(&trx, tree_index)?;

            let origin = scheduled_trx.payer;

            operator_balance.validate_transaction(&trx)?;
            let miner_address = operator_balance.miner(origin);

            log_data(&[b"HASH", &trx.hash]);
            log_data(&[b"MINER", miner_address.as_bytes()]);

            let mut gasometer = Gasometer::new(U256::ZERO, &operator)?;
            gasometer.record_address_lookup_table(accounts);

            let storage = StateAccount::new(
                program_id,
                &holder,
                &accounts_db,
                origin,
                &trx,
                message,
                Some(transaction_tree.pubkey()),
            )?;

            do_scheduled_start(&trx, accounts_db, storage, transaction_tree, gasometer)
        }
        TAG_STATE => Err(Error::ScheduledTxAlreadyInProgress(*holder.key)),
        TAG_SCHEDULED_STATE_FINALIZED | TAG_SCHEDULED_STATE_CANCELLED => {
            Err(Error::StorageAccountFinalized)
        }
        _ => Err(Error::AccountInvalidTag(*holder.key, TAG_HOLDER)),
    }?;

    Ok(())
}
