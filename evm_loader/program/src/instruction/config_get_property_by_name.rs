use solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

use crate::error::Result;

pub fn process<'a>(
    _program_id: &'a Pubkey,
    _accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> Result<()> {
    solana_program::msg!("Instruction: Config Get Property by Name");

    let requested_property = std::str::from_utf8(instruction)?;

    let Ok(index) = crate::config::PARAMETERS.binary_search_by(|p| p.0.cmp(requested_property)) else {
        return Err(ProgramError::InvalidArgument.into());
    };

    let (name, value) = crate::config::PARAMETERS[index];
    assert_eq!(requested_property, name);

    solana_program::program::set_return_data(value.as_bytes());

    Ok(())
}
