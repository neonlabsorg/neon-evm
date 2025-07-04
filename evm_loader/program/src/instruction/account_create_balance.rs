use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, sysvar::Sysvar};

use crate::account::{program, AccountsDB, BalanceAccount, Operator};
use crate::config::CHAIN_ID_LIST;
use crate::error::{Error, Result};
use crate::types::Address;

pub fn process(_program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Create Balance Account");

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0]) }?;
    let system = program::System::from_account_info(&accounts[1])?;

    let accounts_db = AccountsDB::new(&accounts[2..], operator, None, Some(system), None);

    let address = array_ref![instruction, 0, 20];
    let address = Address::from(*address);

    let chain_id = array_ref![instruction, 20, 8];
    let chain_id = u64::from_le_bytes(*chain_id);

    CHAIN_ID_LIST
        .binary_search_by_key(&chain_id, |c| c.0)
        .map_err(|_| Error::InvalidChainId(chain_id))?;

    log_msg!("Address: {}, ChainID: {}", address, chain_id);

    let rent = Rent::get()?;
    BalanceAccount::create(address, chain_id, &accounts_db, None, &rent)?;

    Ok(())
}
