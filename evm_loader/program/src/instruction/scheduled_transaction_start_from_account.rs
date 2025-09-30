use crate::account::{
    AccountRead, Holder, Operator, OperatorBalance, TransactionTree, TAG_HOLDER,
    TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::error::{Error, Result};
use crate::platform::Solana;
use crate::transaction_process::scheduled;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Start Scheduled Transaction from Account");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;

    let holder = accounts[0].clone();
    let tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    match holder.tag(program_id)? {
        TAG_HOLDER => {
            let holder = Holder::from_account(program_id, holder)?;
            holder.validate(&operator)?;

            let rlp = holder.transaction()?;
            let holder_owner = holder.owner();
            let holder_info = holder.into_account();

            let mut solana = Solana::new(&accounts[1..], operator, operator_balance)?;
            solana.use_gasometer(|g| g.record_write_to_holder(&rlp));

            scheduled::start(tree_index, rlp, holder_info, holder_owner, solana, tree)
        }
        TAG_STATE => Err(Error::ScheduledTxAlreadyInProgress(holder.pubkey())),
        TAG_STATE_FINALIZED | TAG_SCHEDULED_STATE_FINALIZED | TAG_SCHEDULED_STATE_CANCELLED => {
            Err(Error::StorageAccountFinalized)
        }
        _ => Err(Error::AccountInvalidTag(holder.pubkey(), TAG_HOLDER)),
    }?;

    Ok(())
}
