use crate::{
    account::{BalanceAccount, Operator, TransactionTree, Treasury},
    error::{Error, Result},
};
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Destroy the Scheduled Transaction.
pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Destroy Transaction Tree Account");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0])? };
    let mut neon_account = BalanceAccount::from_account(program_id, accounts[1].clone())?;
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[2])?;
    let mut tree = TransactionTree::from_account(&crate::ID, accounts[3].clone())?;

    if neon_account.address() != tree.payer() {
        return Err(Error::TreeAccountInvalidPayer);
    }

    if neon_account.chain_id() != tree.chain_id() {
        return Err(Error::TreeAccountInvalidChainId);
    }

    tree.withdraw(&mut neon_account)?;
    tree.destroy(&operator, &treasury)
}
