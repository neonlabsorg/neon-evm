use std::fmt::Display;

use agave_feature_set::FeatureSet;
use mollusk_svm::sysvar::Sysvars;
use num_traits::FromPrimitive;
use solana_loader_v3_interface::state::UpgradeableLoaderState;
use solana_sdk::{account::Account, instruction::InstructionError, pubkey::Pubkey};

use super::error::Error;
use crate::rpc::Rpc;

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum SyncState {
    No,
    Yes,
}

pub async fn download_sysvar_accounts(rpc: &impl Rpc) -> Result<Sysvars, Error> {
    let sysvar_ids = [
        solana_sdk_ids::sysvar::clock::ID,
        solana_sdk_ids::sysvar::epoch_rewards::ID,
        solana_sdk_ids::sysvar::epoch_schedule::ID,
        solana_sdk_ids::sysvar::last_restart_slot::ID,
        solana_sdk_ids::sysvar::rent::ID,
        solana_sdk_ids::sysvar::slot_hashes::ID,
        solana_sdk_ids::sysvar::stake_history::ID,
    ];

    let accounts = rpc.get_multiple_accounts(&sysvar_ids).await?;
    if accounts.iter().any(Option::is_none) {
        return Err(Error::SysvarError);
    }

    let accounts = accounts.into_iter().map(|a| a.unwrap()).collect::<Vec<_>>();

    Ok(Sysvars {
        clock: bincode::deserialize(&accounts[0].data)?,
        epoch_rewards: bincode::deserialize(&accounts[1].data)?,
        epoch_schedule: bincode::deserialize(&accounts[2].data)?,
        last_restart_slot: bincode::deserialize(&accounts[3].data)?,
        rent: bincode::deserialize(&accounts[4].data)?,
        slot_hashes: bincode::deserialize(&accounts[5].data)?,
        stake_history: bincode::deserialize(&accounts[6].data)?,
    })
}

pub async fn download_feature_set(rpc: &impl Rpc) -> Result<FeatureSet, Error> {
    let mut feature_set = FeatureSet::all_enabled();

    let deactivated_features = rpc.get_deactivated_solana_features().await?;
    for feature_id in deactivated_features {
        feature_set.deactivate(&feature_id);
    }

    Ok(feature_set)
}

pub async fn extract_elf(rpc: &impl Rpc, account: Account) -> Result<Vec<u8>, Error> {
    if !account.executable {
        return Err(Error::AccountIsNotProgram);
    }

    match account.owner {
        solana_sdk_ids::bpf_loader::ID => Ok(account.data),
        solana_sdk_ids::bpf_loader_upgradeable::ID => {
            let UpgradeableLoaderState::Program {
                programdata_address,
            } = bincode::deserialize(&account.data)?
            else {
                return Err(Error::AccountIsNotProgram);
            };

            let Some(program_data_account) = rpc.get_account(&programdata_address).await? else {
                return Err(Error::AccountIsNotProgram);
            };

            let start = UpgradeableLoaderState::size_of_programdata_metadata();
            Ok(program_data_account.data[start..].to_vec())
        }
        _ => Err(Error::AccountIsNotProgram),
    }
}

#[must_use]
fn error_code_to_string<E: FromPrimitive + Display>(code: u32) -> String {
    let Some(error) = E::from_u32(code) else {
        return format!("unknown error: {code:#x}");
    };
    error.to_string()
}

#[must_use]
pub fn instruction_error_to_string(program_id: Pubkey, error: InstructionError) -> String {
    use solana_system_interface::error::SystemError;
    use spl_token_interface::error::TokenError;

    match error {
        InstructionError::Custom(code) => match program_id {
            solana_sdk_ids::system_program::ID => error_code_to_string::<SystemError>(code),
            spl_token_interface::ID => error_code_to_string::<TokenError>(code),
            _ => format!("custom program error: {code:#x}"),
        },
        error => error.to_string(),
    }
}
