// use crate::tracing::tracers::state_diff::Account;
use crate::rpc::Rpc;
use async_trait::async_trait;

use bincode::deserialize;
use futures::future::join_all;
use solana_client::client_error::{ClientErrorKind, Result as ClientResult};
use solana_sdk::{
    account::Account, bpf_loader_upgradeable::UpgradeableLoaderState, pubkey::Pubkey,
};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;

use bincode::serialize;
use tokio::sync::OnceCell;
use tracing::info;
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct KeyAccountCache {
    addr: Pubkey,
    slot: u64,
}

use crate::rpc::SliceConfig;
type ProgramDataCache = HashMap<KeyAccountCache, Account>;
type ThreadSaveProgramDataCache = RwLock<ProgramDataCache>;

static LOCAL_CONFIG: OnceCell<ThreadSaveProgramDataCache> = OnceCell::const_new();

pub async fn cut_programdata_from_acc(account: &mut Account, data_slice: SliceConfig) {
    if data_slice.offset != 0 {
        account
            .data
            .drain(..std::cmp::min(account.data.len(), data_slice.offset));
    }
    account.data.truncate(data_slice.length);
}

async fn programdata_hash_get_instance() -> &'static ThreadSaveProgramDataCache {
    LOCAL_CONFIG
        .get_or_init(|| async {
            let map = HashMap::new();

            RwLock::new(map)
        })
        .await
}

async fn programdata_hash_get(addr: Pubkey, slot: u64) -> Option<Account> {
    let val = KeyAccountCache { addr, slot };
    programdata_hash_get_instance()
        .await
        .read()
        .expect("acc_hash_get_instance poisoned")
        .get(&val)
        .cloned()
}

async fn programdata_hash_add(addr: Pubkey, slot: u64, acc: Account) {
    let val = KeyAccountCache { addr, slot };
    programdata_hash_get_instance()
        .await
        .write()
        .expect("PANIC, no nable")
        .insert(val, acc);
}

fn get_programdata_slot_from_account(acc: &Account) -> ClientResult<u64> {
    if !bpf_loader_upgradeable::check_id(&acc.owner) {
        return Err(ClientErrorKind::Custom("Not upgradeable account".to_string()).into());
    }

    match deserialize::<UpgradeableLoaderState>(&acc.data) {
        Ok(UpgradeableLoaderState::ProgramData { slot, .. }) => Ok(slot),
        Ok(_) => {
            panic!("Account is not of type `ProgramData`.");
        }
        Err(e) => {
            eprintln!("Error occurred: {e:?}");
            panic!("Failed to deserialize account data.");
        }
    }
}

pub async fn programdata_cache_get_values_by_keys(
    programdata_keys: &Vec<Pubkey>,
    rpc: &impl Rpc,
) -> ClientResult<Vec<Option<solana_sdk::account::Account>>> {
    let mut future_requests = Vec::new();
    let mut answer = Vec::new();

    for key in programdata_keys {
        future_requests.push(rpc.get_account_slice(
            key,
            Some(SliceConfig {
                offset: 0,
                length: UpgradeableLoaderState::size_of_programdata_metadata(),
            }),
        ));
    }

    assert_eq!(
        programdata_keys.len(),
        future_requests.len(),
        "programdata_keys.size()!=future_requests.size()"
    );
    let results = join_all(future_requests).await;
    for (result, key) in results.iter().zip(programdata_keys) {
        match result {
            Ok(Some(account)) => {
                let slot_val = get_programdata_slot_from_account(account)?;
                if let Some(acc) = programdata_hash_get(*key, slot_val).await {
                    answer.push(Some(acc));
                } else if let Ok(Some(tmp_acc)) = rpc.get_account(key).await {
                    let current_slot = get_programdata_slot_from_account(&tmp_acc)?;
                    programdata_hash_add(*key, current_slot, tmp_acc.clone()).await;

                    answer.push(Some(tmp_acc));
                } else {
                    answer.push(None);
                }
            }
            Ok(None) => {
                info!("Account for key {key:?} is None.");
                answer.push(None);
            }
            Err(e) => {
                info!("Error fetching account for key {key:?}: {e:?}");
            }
        }
    }
    Ok(answer)
}

struct FakeRpc {
    accounts: HashMap<Pubkey, Account>,
}
#[allow(dead_code)]
impl FakeRpc {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    fn has_account(&self, pubkey: &Pubkey) -> bool {
        self.accounts.contains_key(pubkey)
    }

    fn make_account(&mut self, pubkey: Pubkey) -> Account {
        // Define the slot number you want to test with
        let test_slot: u64 = 42;

        // Create mock ProgramData state
        let program_data = UpgradeableLoaderState::ProgramData {
            slot: test_slot,
            upgrade_authority_address: Some(Pubkey::new_unique()),
        };
        let mut serialized_data = serialize(&program_data).unwrap();
        serialized_data.resize(4 * 1024 * 1024, 0);
        let mut answer = Account::new(0, serialized_data.len(), &bpf_loader_upgradeable::id());
        answer.data = serialized_data;

        self.accounts.insert(pubkey, answer.clone());
        answer
    }
}

#[async_trait(?Send)]

impl Rpc for FakeRpc {
    async fn get_account_slice(
        &self,
        pubkey: &Pubkey,
        slice: Option<SliceConfig>,
    ) -> ClientResult<Option<Account>> {
        assert!(self.accounts.contains_key(pubkey), "  ");

        let mut answer = self.accounts.get(pubkey).unwrap().clone();
        if let Some(data_slice) = slice {
            if data_slice.offset != 0 {
                answer
                    .data
                    .drain(..std::cmp::min(answer.data.len(), data_slice.offset));
            }
            answer.data.truncate(data_slice.length);
        }
        Ok(Some(answer))
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        let mut futures = Vec::new();
        for pubkey in pubkeys {
            futures.push(self.get_account(pubkey).await?);
        }

        Ok(futures)
    }

    async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
        Ok(Vec::new())
    }
}
use evm_loader::solana_program::bpf_loader_upgradeable;
use tokio;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acc_slice() {
        let mut rpc = FakeRpc::new();
        let test_key = Pubkey::new_unique();

        let test_acc = rpc.make_account(test_key);
        if test_acc.data.len() >= 4000000 {
            println!("Account data len: {}", test_acc.data.len());
        } else {
            panic!("test stop");
        }
        if let Ok(test2_acc) = rpc.get_account(&test_key).await {
            assert_eq!(
                test_acc.data.len(),
                test2_acc.expect("test fail").data.len()
            );
        } else {
            panic!("fake rpc returned error");
        }

        let test3_acc = rpc
            .get_account_slice(
                &test_key,
                Some(SliceConfig {
                    offset: 0,
                    length: 1024,
                }),
            )
            .await;
        assert_eq!(1024, test3_acc.unwrap().expect("test fail").data.len());
    }
    #[tokio::test]
    async fn test_acc_request() {
        const TEST_KEYS_COUNT: usize = 10;
        let mut rpc = FakeRpc::new();
        let mut test_keys = Vec::new(); //Pubkey::new_unique();

        for _i in 0..TEST_KEYS_COUNT {
            let curr_key = Pubkey::new_unique();
            rpc.make_account(curr_key);
            test_keys.push(curr_key);
        }

        let multiple_accounts = rpc
            .get_multiple_accounts(&test_keys)
            .await
            .expect("ERR DURING ACC REQUESTS");

        let hashed_accounts = programdata_cache_get_values_by_keys(&test_keys, &rpc)
            .await
            .expect("ERR DURING ACC REQUESTS WITH HASH");
        assert_eq!(hashed_accounts.len(), multiple_accounts.len());
        for i in 0..TEST_KEYS_COUNT {
            assert!(hashed_accounts[i].is_some(), "BAD ACC");
            assert!(multiple_accounts[i].is_some(), "BAD ACC");
        }
    }
}
