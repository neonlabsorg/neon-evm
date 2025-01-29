#![allow(clippy::cast_possible_truncation)]

use std::collections::BTreeMap;

use crate::executor::OwnedAccountInfo;
use solana_program::{
    entrypoint::ProgramResult, instruction::AccountMeta, program_error::ProgramError,
    pubkey::Pubkey, system_instruction::SystemInstruction, system_program,
};

pub fn emulate(
    instruction: &[u8],
    meta: &[AccountMeta],
    accounts: &mut BTreeMap<Pubkey, OwnedAccountInfo>,
) -> ProgramResult {
    let system_instruction: SystemInstruction = bincode::deserialize(instruction).unwrap();
    match system_instruction {
        SystemInstruction::CreateAccount {
            lamports,
            space,
            owner,
        } => {
            let funder_key = &meta[0].pubkey;
            let account_key = &meta[1].pubkey;

            {
                let funder = accounts.get_mut(funder_key).unwrap();
                if funder.lamports < lamports {
                    return Err(ProgramError::InsufficientFunds);
                }

                funder.lamports -= lamports;
            }

            {
                let account = accounts.get_mut(account_key).unwrap();
                if (account.lamports > 0)
                    || !account.data.is_empty()
                    || !system_program::check_id(&account.owner)
                {
                    return Err(ProgramError::AccountAlreadyInitialized);
                }

                account.lamports = lamports;
                account.owner = owner;
                account.data.resize(space as usize, 0_u8);
            }
        }
        SystemInstruction::Assign { owner } => {
            let account_key = &meta[0].pubkey;
            let account = accounts.get_mut(account_key).unwrap();

            if !system_program::check_id(&account.owner) {
                return Err(ProgramError::AccountAlreadyInitialized);
            }

            account.owner = owner;
        }
        SystemInstruction::Transfer { lamports } => {
            let from_key = &meta[0].pubkey;
            let to_key = &meta[1].pubkey;

            {
                let from = accounts.get_mut(from_key).unwrap();
                if !from.data.is_empty() {
                    return Err(ProgramError::InvalidArgument);
                }

                if from.lamports < lamports {
                    return Err(ProgramError::InsufficientFunds);
                }

                if !system_program::check_id(&from.owner) {
                    return Err(ProgramError::InsufficientFunds);
                }

                from.lamports -= lamports;
            }

            {
                let to = accounts.get_mut(to_key).unwrap();
                to.lamports += lamports;
            }
        }
        SystemInstruction::Allocate { space } => {
            let account_key = &meta[0].pubkey;
            let account = accounts.get_mut(account_key).unwrap();

            if !account.data.is_empty() || !system_program::check_id(&account.owner) {
                return Err(ProgramError::InvalidInstructionData);
            }

            account.data.resize(space as usize, 0_u8);
        }
        _ => {
            return Err(ProgramError::InvalidInstructionData);
        }
    }

    Ok(())
}
