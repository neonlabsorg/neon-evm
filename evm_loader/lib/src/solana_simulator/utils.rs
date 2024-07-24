use super::error::Error;
use log::debug;
use solana_program_runtime::sysvar_cache::SysvarCache;
use solana_sdk::{
    account::Account,
    account_utils::StateMut,
    address_lookup_table::{
        self,
        state::{AddressLookupTable, LookupTableMeta},
    },
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    pubkey::Pubkey,
    sysvar,
};

use crate::rpc::Rpc;

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum SyncState {
    No,
    Yes,
}

pub async fn sync_sysvar_accounts(
    rpc: &impl Rpc,
    sysvar_cache: &mut SysvarCache,
) -> Result<(), Error> {
    let keys = sysvar::ALL_IDS.clone();
    let accounts = rpc.get_multiple_accounts(&keys).await?;
    for (key, account) in keys.into_iter().zip(accounts) {
        let Some(account) = account else {
            continue;
        };

        match key {
            sysvar::clock::ID => {
                use sysvar::clock::Clock;

                let clock: Clock = bincode::deserialize(&account.data)?;
                sysvar_cache.set_clock(clock);
            }
            sysvar::epoch_rewards::ID => {
                use sysvar::epoch_rewards::EpochRewards;

                let epoch_rewards: EpochRewards = bincode::deserialize(&account.data)?;
                sysvar_cache.set_epoch_rewards(epoch_rewards);
            }
            sysvar::epoch_schedule::ID => {
                use sysvar::epoch_schedule::EpochSchedule;

                let epoch_schedule: EpochSchedule = bincode::deserialize(&account.data)?;
                sysvar_cache.set_epoch_schedule(epoch_schedule);
            }
            sysvar::rent::ID => {
                use sysvar::rent::Rent;

                let rent: Rent = bincode::deserialize(&account.data)?;
                sysvar_cache.set_rent(rent);
            }
            sysvar::slot_hashes::ID => {
                use sysvar::slot_hashes::SlotHashes;

                let slot_hashes: SlotHashes = bincode::deserialize(&account.data)?;
                sysvar_cache.set_slot_hashes(slot_hashes);
            }
            sysvar::stake_history::ID => {
                use sysvar::stake_history::StakeHistory;

                let stake_history: StakeHistory = bincode::deserialize(&account.data)?;
                sysvar_cache.set_stake_history(stake_history);
            }
            #[allow(deprecated)]
            id if sysvar::fees::check_id(&id) => {
                use sysvar::fees::Fees;

                let fees: Fees = bincode::deserialize(&account.data)?;
                sysvar_cache.set_fees(fees);
            }
            sysvar::last_restart_slot::ID => {
                use sysvar::last_restart_slot::LastRestartSlot;

                let last_restart_slot: LastRestartSlot = bincode::deserialize(&account.data)?;
                sysvar_cache.set_last_restart_slot(last_restart_slot);
            }
            #[allow(deprecated)]
            id if sysvar::recent_blockhashes::check_id(&id) => {
                use sysvar::recent_blockhashes::RecentBlockhashes;

                let recent_blockhashes: RecentBlockhashes = bincode::deserialize(&account.data)?;
                sysvar_cache.set_recent_blockhashes(recent_blockhashes);
            }
            _ => {}
        }
    }

    Ok(())
}

pub fn program_data_address(account: &Account) -> Result<Pubkey, Error> {
    assert!(account.executable);
    assert_eq!(account.owner, bpf_loader_upgradeable::id());

    let UpgradeableLoaderState::Program {
        programdata_address,
        ..
    } = account.state()?
    else {
        return Err(Error::ProgramAccountError);
    };

    Ok(programdata_address)
}

pub fn reset_program_data_slot(account: &mut Account) -> Result<(), Error> {
    assert_eq!(account.owner, bpf_loader_upgradeable::id());

    let UpgradeableLoaderState::ProgramData {
        slot,
        upgrade_authority_address,
    } = account.state()?
    else {
        return Err(Error::ProgramAccountError);
    };

    debug!(
        "slot_before_update: slot={slot} upgrade_authority_address={upgrade_authority_address:?}"
    );

    let new_state = UpgradeableLoaderState::ProgramData {
        slot: 0,
        upgrade_authority_address,
    };
    account.set_state(&new_state)?;

    debug!(
        "slot_after_update: slot={slot} upgrade_authority_address={upgrade_authority_address:?}"
    );

    Ok(())
}

pub fn reset_alt_slot(account: &mut Account) -> Result<(), Error> {
    assert_eq!(account.owner, address_lookup_table::program::id());

    let lookup_table = AddressLookupTable::deserialize(&account.data)?;
    let metadata = LookupTableMeta {
        last_extended_slot: 0,
        ..lookup_table.meta
    };

    AddressLookupTable::overwrite_meta_data(&mut account.data, metadata)?;

    Ok(())
}
