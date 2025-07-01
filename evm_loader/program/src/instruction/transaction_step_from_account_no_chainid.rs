use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Begin or Continue Transaction from Account Without ChainId");

    super::transaction_step_from_account::process_inner(program_id, accounts, instruction, true)
}
