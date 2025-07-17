use crate::account::{
    AccountDispatch, Operator, OperatorBalance, TransactionTree, TAG_HOLDER,
    TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE, TAG_STATE_FINALIZED,
};
use crate::error::{Error, Result};
use crate::instruction::transaction_step_from_instruction;
use crate::platform::Solana;
use crate::transaction_process::scheduled;
use crate::types::EncodedTransaction;
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Start Scheduled Transaction from Instruction");

    let tree_index = u16::try_from(u32::from_le_bytes(*array_ref![instruction, 0, 4]))?;
    let message = &instruction[4..];

    let holder = accounts[0].clone();
    let tree = TransactionTree::from_account_info(program_id, &accounts[1])?;
    let operator = Operator::from_account_info(&accounts[2])?;
    let operator_balance = OperatorBalance::try_from_account_info(program_id, &accounts[3])?;

    match holder.tag(program_id)? {
        TAG_HOLDER | TAG_STATE_FINALIZED => {
            let rlp = EncodedTransaction::from_rlp(message);

            let (holder, holder_owner) = transaction_step_from_instruction::validate_holder(
                program_id, holder, &operator, &rlp,
            )?;

            let solana = Solana::new(accounts, operator, operator_balance)?;
            scheduled::start(tree_index, rlp, holder, holder_owner, solana, tree)
        }
        TAG_STATE => Err(Error::ScheduledTxAlreadyInProgress(holder.pubkey())),
        TAG_SCHEDULED_STATE_FINALIZED | TAG_SCHEDULED_STATE_CANCELLED => {
            Err(Error::StorageAccountFinalized)
        }
        _ => Err(Error::AccountInvalidTag(holder.pubkey(), TAG_HOLDER)),
    }
}
