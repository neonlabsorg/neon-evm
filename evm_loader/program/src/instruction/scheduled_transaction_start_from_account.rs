use crate::account::legacy::{TAG_HOLDER_DEPRECATED, TAG_STATE_FINALIZED_DEPRECATED};
use crate::account::{
    AccountsDB, Operator, OperatorBalanceAccount, OperatorBalanceValidator, StateAccount,
    TransactionTree, TAG_HOLDER, TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED,
    TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::debug::log_data;
use crate::error::{Error, Result};
use crate::gasometer::Gasometer;
use crate::instruction::scheduled_transaction_start::do_scheduled_start;
use crate::instruction::transaction_internals::holder_parse_trx;
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Start Scheduled Transaction from Account");

    let holder = accounts[0].clone();
    let transaction_tree = TransactionTree::from_account(&program_id, accounts[1].clone())?;
    let operator = Operator::from_account(&accounts[2])?;
    let operator_balance = OperatorBalanceAccount::try_from_account(program_id, &accounts[3])?;

    let accounts_db = AccountsDB::new(
        &accounts[4..],
        operator.clone(),
        operator_balance.clone(),
        None,
        None,
    );

    let mut excessive_lamports = 0_u64;

    let mut tag = crate::account::tag(program_id, &holder)?;
    if (tag == TAG_HOLDER_DEPRECATED) || (tag == TAG_STATE_FINALIZED_DEPRECATED) {
        tag = crate::account::legacy::update_holder_account(&holder)?;
    }

    match tag {
        TAG_HOLDER => {
            let trx = holder_parse_trx(holder.clone(), &operator, program_id)?;

            // Validate that it's indeed a scheduled tx.
            if !trx.is_scheduled_tx() {
                return Err(Error::NotScheduledTransaction);
            }

            let origin = trx.if_scheduled().unwrap().payer;

            operator_balance.validate_transaction(&trx)?;
            let miner_address = operator_balance.miner(origin);

            log_data(&[b"HASH", &trx.hash]);
            log_data(&[b"MINER", miner_address.as_bytes()]);

            let mut gasometer = Gasometer::new(U256::ZERO, &operator)?;
            gasometer.record_solana_transaction_cost();
            gasometer.record_address_lookup_table(accounts);
            gasometer.record_write_to_holder(&trx);

            excessive_lamports += crate::account::legacy::update_legacy_accounts(&accounts_db)?;
            gasometer.refund_lamports(excessive_lamports);

            let storage = StateAccount::new(
                program_id,
                holder,
                &accounts_db,
                origin,
                trx,
                Some(*transaction_tree.info().key),
            )?;

            do_scheduled_start(accounts_db, storage, transaction_tree, gasometer)
        }
        TAG_STATE => Err(Error::ScheduledTxAlreadyInProgress(*holder.key)),
        TAG_STATE_FINALIZED | TAG_SCHEDULED_STATE_FINALIZED | TAG_SCHEDULED_STATE_CANCELLED => {
            Err(Error::StorageAccountFinalized)
        }
        _ => Err(Error::AccountInvalidTag(*holder.key, TAG_HOLDER)),
    }?;

    **operator.try_borrow_mut_lamports()? += excessive_lamports;

    Ok(())
}
