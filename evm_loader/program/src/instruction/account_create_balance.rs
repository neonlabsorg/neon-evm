use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::Operator;
use crate::error::{Error, Result};
use crate::platform::{Platform, Solana};
use crate::types::Address;

pub fn process(_program_id: Pubkey, accounts: &[AccountInfo], instruction: &[u8]) -> Result<()> {
    log_msg!("Instruction: Create Balance Account");

    let address = array_ref![instruction, 0, 20];
    let address = Address::from(*address);

    let chain_id = array_ref![instruction, 20, 8];
    let chain_id = u64::from_le_bytes(*chain_id);

    log_msg!("Address: {}, ChainID: {}", address, chain_id);

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0]) }?;
    let mut solana = Solana::new(accounts, operator, None)?;

    if !solana.chains().any(|c| c.id == chain_id) {
        return Err(Error::InvalidChainId(chain_id));
    };

    solana.create_balance(address, chain_id)?;
    solana.update_accounts_lamports()
}
