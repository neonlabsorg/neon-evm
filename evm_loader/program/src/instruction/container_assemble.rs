use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, sysvar::Sysvar};

use crate::{
    account::{program::System, ContainerAccount, Operator, Treasury},
    error::Result,
};

pub fn process(program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Assemble Container");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let operator = Operator::from_account_info(&accounts[0])?;
    let _treasury = Treasury::from_account_info(program_id, treasury_index, &accounts[1])?;
    let system = System::from_account_info(&accounts[2])?;
    let container_info = &accounts[3];
    let accounts = &accounts[4..];

    // Assemble the container account
    let container = container_info.clone().into();
    let mut container = ContainerAccount::convert_from_account(program_id, container)?;

    for account_info in accounts {
        let account = account_info.clone().into();
        unsafe { container.add_account(program_id, account) }?;
    }

    // Transfer lamports to the container account
    let rent = Rent::get()?;
    let minimum_balance = rent.minimum_balance(container_info.data_len());
    let required_lamports = minimum_balance.saturating_sub(container_info.lamports());

    if required_lamports > 0 {
        system.transfer(&operator, container_info, required_lamports)?;
    }

    Ok(())
}
