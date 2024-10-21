use crate::error::Result;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

pub fn process<'a>(
    _program_id: &'a Pubkey,
    _accounts: &'a [AccountInfo<'a>],
    _instruction: &[u8],
) -> Result<()> {
    Ok(())
}
