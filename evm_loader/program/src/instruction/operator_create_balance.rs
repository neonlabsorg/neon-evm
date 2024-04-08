use arrayref::array_ref;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, sysvar::Sysvar};

use crate::account::{program, Operator, OperatorBalanceAccount};
use crate::config::CHAIN_ID_LIST;
use crate::error::{Error, Result};
use crate::types::Address;

pub fn process<'a>(
    _program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    log_msg!("Instruction: Create Operator Balance Account");

    let operator = unsafe { Operator::from_account_not_whitelisted(&accounts[0]) }?;
    let system = program::System::from_account(&accounts[1])?;
    let account = &accounts[2];

    let address = array_ref![instruction, 0, 20];
    let address = Address::from(*address);

    let chain_id = array_ref![instruction, 20, 8];
    let chain_id = u64::from_le_bytes(*chain_id);

    CHAIN_ID_LIST
        .binary_search_by_key(&chain_id, |c| c.0)
        .map_err(|_| Error::InvalidChainId(chain_id))?;

    log_msg!("Address: {}, ChainID: {}", address, chain_id);

    let rent = Rent::get()?;
    OperatorBalanceAccount::create(address, chain_id, account, &operator, &system, &rent)?;

    Ok(())
}
