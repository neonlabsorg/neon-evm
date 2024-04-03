use crate::account::{program::System, MainTreasury, Treasury};
use crate::pda_seeds::with_treasury_seeds;
use arrayref::array_ref;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::invoke_signed, pubkey::Pubkey,
    rent::Rent, system_instruction, sysvar::Sysvar,
};

pub fn process<'a>(
    program_id: &'a Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction: &[u8],
) -> ProgramResult {
    log_msg!("Instruction: Collect treasury");

    let treasury_index = u32::from_le_bytes(*array_ref![instruction, 0, 4]);

    let main_treasury = MainTreasury::from_account(program_id, &accounts[0])?;
    let treasury = Treasury::from_account(program_id, treasury_index, &accounts[1])?;
    let system = System::from_account(&accounts[2])?;

    let rent = Rent::get()?;
    let minimal_balance_for_rent_exempt = rent.minimum_balance(treasury.data_len());
    let available_lamports = treasury
        .lamports()
        .saturating_sub(minimal_balance_for_rent_exempt);

    if available_lamports > 0 {
        with_treasury_seeds(treasury_index, &[treasury.get_bump_seed()], |seeds| {
            invoke_signed(
                &system_instruction::transfer(treasury.key, main_treasury.key, available_lamports),
                &[treasury.clone(), main_treasury.clone(), system.clone()],
                &[seeds],
            )
        })?;
    };

    Ok(())
}
