use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, sysvar::Sysvar};

use crate::{
    account::{program::System, ContainerAccount, Operator, ReferenceAccount, Treasury},
    error::Result,
};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Disassemble Container");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let operator = Operator::from_account_info(&accounts[0])?;
    let _treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[1])?;
    let system = System::from_account_info(&accounts[2])?;
    let mut container = ContainerAccount::from_account_info(program_id, &accounts[3])?;

    // Remove accounts from container
    for account in &accounts[4..] {
        let reference = ReferenceAccount::from_account_info(program_id, account)?;
        unsafe { container.remove_account(reference) }?;
    }

    if container.count() == 1 {
        container.unwrap_container()?;
    }

    let rent = Rent::get()?;

    // Pop up lamports to accounts
    for account in &accounts[3..] {
        let minimum_balance = rent.minimum_balance(account.data_len());
        if minimum_balance > account.lamports() {
            let required_lamports = minimum_balance - account.lamports();
            system.transfer(&operator, account, required_lamports)?;
        }
    }

    // Remove excessive lamports from accounts
    // Do it in separate loop because we can't do transfer from operator after popping up it's balance
    for account in &accounts[3..] {
        let minimum_balance = rent.minimum_balance(account.data_len());
        if minimum_balance < account.lamports() {
            let excessive_lamports = account.lamports() - minimum_balance;

            **account.lamports.borrow_mut() -= excessive_lamports;
            **operator.lamports.borrow_mut() += excessive_lamports;
        }
    }

    Ok(())
}
