// use crate::tracing::tracers::state_diff::Account;
use crate::rpc::Rpc;
use async_trait::async_trait;

use crate::commands::get_config::GetConfigResponse;
use bincode::deserialize;
use futures::future::join_all;
use solana_client::client_error::Result as ClientResult;
use solana_sdk::{
    account::Account, bpf_loader_upgradeable::UpgradeableLoaderState, pubkey::Pubkey,
};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;

use bincode::serialize;
use tokio::sync::OnceCell;
use tracing::info;

use crate::rpc::SliceConfig;
#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct KeyAccountCache {
    pub addr: Pubkey,
    pub slot: u64,
}
impl KeyAccountCache {
    #[must_use]

    pub const fn new(addr: &Pubkey, slot: u64) -> Self {
        Self { addr: *addr, slot }
    }
}

type ProgramDataCache<Value> = HashMap<KeyAccountCache, Value>;

struct ThreadSaveCache<Value>
where
    Value: Clone,
{
    table: RwLock<ProgramDataCache<Value>>,
}
impl<Value> ThreadSaveCache<Value>
where
    Value: Clone,
{
    pub fn new() -> Self {
        Self {
            table: RwLock::new(HashMap::new()),
        }
    }

    fn get(&self, key: &KeyAccountCache) -> Option<Value> {
        self.table
            .read()
            .expect("lock on read error ")
            .get(key)
            .cloned()
    }
    fn add(&self, key: KeyAccountCache, value: Value) {
        self.table
            .write()
            .expect("lock on write error  ")
            .insert(key, value);
    }
}

type ThreadSaveProgramDataCache = ThreadSaveCache<Account>;
type ThreadSaveConfigCache = ThreadSaveCache<GetConfigResponse>;

static ACCOUNT_CACHE_TABLE: OnceCell<ThreadSaveProgramDataCache> = OnceCell::const_new();
static CONFIG_CACHE_TABLE: OnceCell<ThreadSaveConfigCache> = OnceCell::const_new();

pub async fn cut_programdata_from_acc(account: &mut Account, data_slice: SliceConfig) {
    if data_slice.offset != 0 {
        account
            .data
            .drain(..std::cmp::min(account.data.len(), data_slice.offset));
    }
    account.data.truncate(data_slice.length);
}

async fn programdata_account_cache_get_instance() -> &'static ThreadSaveProgramDataCache {
    ACCOUNT_CACHE_TABLE
        .get_or_init(|| async { ThreadSaveProgramDataCache::new() })
        .await
}

async fn programdata_account_cache_get(key: &KeyAccountCache) -> Option<Account> {
    programdata_account_cache_get_instance().await.get(key)
}

async fn programdata_account_cache_add(key: KeyAccountCache, acc: Account) {
    programdata_account_cache_get_instance().await.add(key, acc);
}

/// in case of Not upgradeable account - return option None
pub fn get_program_programdata_address(acc: &Account) -> ClientResult<Option<Pubkey>> {
    assert!(!bpf_loader_upgradeable::check_id(&acc.owner), "NOT AN ACC");

    match deserialize::<UpgradeableLoaderState>(&acc.data) {
        Ok(UpgradeableLoaderState::Program {
            programdata_address,
            ..
        }) => Ok(Some(programdata_address)),
        Ok(_) => {
            panic!("Unexpected account type! Only Program type is acceptable  ");
        }
        Err(e) => {
            eprintln!("Error occurred: {e:?}");
            panic!("Failed to deserialize account data.");
        }
    }
}

pub fn get_programdata_slot_from_account(acc: &Account) -> ClientResult<Option<u64>> {
    assert!(bpf_loader_upgradeable::check_id(&acc.owner), "NOT AN ACC");
    match deserialize::<UpgradeableLoaderState>(&acc.data) {
        Ok(UpgradeableLoaderState::ProgramData { slot, .. }) => Ok(Some(slot)),
        Ok(UpgradeableLoaderState::Program {
            programdata_address,
            ..
        }) => {
            info!(" programdata_address:{programdata_address}");
            Ok(Some(0))
        }

        Ok(_) => {
            panic!("Unexpected account type! Only ProgramData type is acceptable   ");
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
    for (result, addr) in results.iter().zip(programdata_keys) {
        match result {
            Ok(Some(account)) => {
                if let Some(slot_val) = get_programdata_slot_from_account(account)? {
                    let key = KeyAccountCache::new(addr, slot_val);
                    if let Some(acc) = programdata_account_cache_get(&key).await {
                        answer.push(Some(acc));
                    } else if let Ok(Some(tmp_acc)) = rpc.get_account(&key.addr).await {
                        let current_slot =
                            get_programdata_slot_from_account(&tmp_acc)?.expect("No current slot ");
                        let key = KeyAccountCache::new(addr, current_slot);
                        programdata_account_cache_add(key, tmp_acc.clone()).await;

                        answer.push(Some(tmp_acc));
                    } else {
                        answer.push(None);
                    }
                } else {
                    answer.push(None);
                }
            }
            Ok(None) => {
                info!("Account for key {addr:?} is None.");
                answer.push(None);
            }
            Err(e) => {
                info!("Error fetching account for key {addr:?}: {e:?}");
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

async fn program_config_cache_get_instance() -> &'static ThreadSaveConfigCache {
    CONFIG_CACHE_TABLE
        .get_or_init(|| async { ThreadSaveConfigCache::new() })
        .await
}

pub async fn program_config_cache_get(key: &KeyAccountCache) -> Option<GetConfigResponse> {
    program_config_cache_get_instance().await.get(key)
}

pub async fn program_config_cache_add(key: KeyAccountCache, val: GetConfigResponse) {
    program_config_cache_get_instance().await.add(key, val);
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

    #[test]
    fn test_create_new_cache() {
        let cache: ThreadSaveCache<String> = ThreadSaveCache::new();
        assert!(cache
            .get(&KeyAccountCache {
                slot: 0,
                addr: Pubkey::new_unique(),
            })
            .is_none());
    }

    #[test]
    fn test_get_nonexistent_key() {
        let cache: ThreadSaveCache<String> = ThreadSaveCache::new();
        let key = KeyAccountCache {
            slot: 0,
            addr: Pubkey::new_unique(),
        };

        // Attempt to get a value for a key that doesn't exist
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_overwrite_existing_key() {
        let cache: ThreadSaveCache<String> = ThreadSaveCache::new();
        let key = KeyAccountCache {
            slot: 0,
            addr: Pubkey::new_unique(),
        };
        let value1 = "value1".to_string();
        let value2 = "value2".to_string();

        // Add the first value
        cache.add(key.clone(), value1.clone());
        assert_eq!(cache.get(&key).unwrap(), value1);

        // Overwrite with the second value
        cache.add(key.clone(), value2.clone());
        assert_eq!(cache.get(&key).unwrap(), value2);
    }

    #[test]
    fn test_multiple_keys() {
        let cache: ThreadSaveCache<String> = ThreadSaveCache::new();
        let key1 = KeyAccountCache {
            slot: 0,
            addr: Pubkey::new_unique(),
        };
        let value1 = "value1".to_string();
        let key2 = KeyAccountCache {
            slot: 0,
            addr: Pubkey::new_unique(),
        };
        let value2 = "value2".to_string();

        // Add multiple key-value pairs
        cache.add(key1.clone(), value1.clone());
        cache.add(key2.clone(), value2.clone());

        // Check values for both keys
        assert_eq!(cache.get(&key1).unwrap(), value1);
        assert_eq!(cache.get(&key2).unwrap(), value2);
    }
}
