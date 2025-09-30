use crate::{
    account::{Operator, TransactionTree, Treasury},
    error::Result,
    platform::{Platform, Solana},
};
use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

/// Destroy the Scheduled Transaction.
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Destroy Transaction Tree Account");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0])? };
    // let mut payer = BalanceAccount::from_account_info(program_id, &accounts[1])?;
    let treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[2])?;
    let mut tree = TransactionTree::from_account_info(program_id, &accounts[3])?;

    let mut solana = Solana::new(accounts, operator.clone(), None)?;
    let mut payer = solana.create_balance(tree.payer(), tree.chain_id())?;

    tree.withdraw(&mut payer)?;
    tree.destroy(&operator, &treasury)?;

    solana.update_accounts_lamports()
}
