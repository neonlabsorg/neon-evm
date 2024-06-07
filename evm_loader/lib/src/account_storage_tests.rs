use super::*;
use crate::rpc;
use crate::tracing::AccountOverride;
use hex_literal::hex;
use std::collections::HashMap;
use std::str::FromStr;

const STORAGE_LENGTH: usize = 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;

mod mock_rpc_client {
    use crate::commands::get_config::BuildConfigSimulator;
    use crate::NeonResult;
    use crate::{commands::get_config::ConfigSimulator, rpc::Rpc};
    use async_trait::async_trait;
    use solana_client::client_error::Result as ClientResult;
    use solana_sdk::account::Account;
    use solana_sdk::clock::{Slot, UnixTimestamp};
    use solana_sdk::pubkey::Pubkey;
    use std::collections::HashMap;

    pub struct MockRpcClient {
        accounts: HashMap<Pubkey, Account>,
    }

    impl MockRpcClient {
        pub fn new(accounts: &[(Pubkey, Account)]) -> Self {
            Self {
                accounts: accounts.iter().cloned().collect(),
            }
        }
    }

    #[async_trait(?Send)]
    impl Rpc for MockRpcClient {
        async fn get_account(&self, key: &Pubkey) -> ClientResult<Option<Account>> {
            let result = self.accounts.get(key).cloned();
            Ok(result)
        }

        async fn get_multiple_accounts(
            &self,
            pubkeys: &[Pubkey],
        ) -> ClientResult<Vec<Option<Account>>> {
            let result = pubkeys
                .iter()
                .map(|key| self.accounts.get(key).cloned())
                .collect::<Vec<_>>();
            Ok(result)
        }

        async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
            Ok(UnixTimestamp::default())
        }

        async fn get_slot(&self) -> ClientResult<Slot> {
            Ok(Slot::default())
        }

        async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
            Ok(vec![])
        }
    }

    #[async_trait(?Send)]
    impl BuildConfigSimulator for MockRpcClient {
        fn use_cache(&self) -> bool {
            false
        }
        async fn build_config_simulator(&self, _program_id: Pubkey) -> NeonResult<ConfigSimulator> {
            unimplemented!();
        }
    }
}

async fn get_overriden_nonce_and_balance(
    address: Address,
    tx_chain_id: u64,
    nonce_chain_id: u64,
    overrides: Option<AccountOverrides>,
) -> (u64, U256) {
    let mut fixture = Fixture::new();
    fixture.state_overrides = overrides;
    let storage = fixture
        .build_account_storage_with_chain_id(Some(tx_chain_id))
        .await;

    (
        storage.nonce(address, nonce_chain_id).await,
        storage.balance(address, nonce_chain_id).await,
    )
}

async fn get_balance_account_info<T: rpc::Rpc, F, R>(
    storage: &EmulatorAccountStorage<'_, T>,
    action: F,
) -> NeonResult<R>
where
    F: FnOnce(&BalanceAccount) -> R,
{
    let mut balance_data = storage
        .get_balance_account(ACTUAL_BALANCE.address, LEGACY_CHAIN_ID)
        .await?
        .borrow_mut();
    let balance_account =
        BalanceAccount::from_account(&storage.program_id, balance_data.into_account_info());

    Ok(action(&balance_account?))
}

#[allow(clippy::too_many_arguments)]
fn create_legacy_ether_contract(
    program_id: &Pubkey,
    rent: &Rent,
    address: Address,
    balance: U256,
    trx_count: u64,
    generation: u32,
    code: &[u8],
    storage: &[[u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],
) -> Account {
    let data_length = if (!code.is_empty()) || (generation > 0) {
        1 + LegacyEtherData::SIZE + 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT + code.len()
    } else {
        1 + LegacyEtherData::SIZE
    };
    let mut data = vec![0u8; data_length];

    let data_ref = arrayref::array_mut_ref![data, 0, 1 + LegacyEtherData::SIZE];
    let (
        tag_ptr,
        address_ptr,
        bump_seed_ptr,
        trx_count_ptr,
        balance_ptr,
        generation_ptr,
        code_size_ptr,
        rw_blocked_ptr,
    ) = arrayref::mut_array_refs![data_ref, 1, 20, 1, 8, 32, 4, 4, 1];

    *tag_ptr = LegacyEtherData::TAG.to_le_bytes();
    *address_ptr = *address.as_bytes();
    *bump_seed_ptr = 0u8.to_le_bytes();
    *trx_count_ptr = trx_count.to_le_bytes();
    *balance_ptr = balance.to_le_bytes();
    *generation_ptr = generation.to_le_bytes();
    *code_size_ptr = u32::try_from(code.len())
        .expect("Expected code value")
        .to_le_bytes();
    *rw_blocked_ptr = 0u8.to_le_bytes();

    if (generation > 0) || (!code.is_empty()) {
        let storage_offset = 1 + LegacyEtherData::SIZE;

        let storage_ptr = &mut data[storage_offset..][..STORAGE_LENGTH];
        let storage_source = unsafe {
            let ptr: *const u8 = storage.as_ptr().cast();
            std::slice::from_raw_parts(ptr, 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT)
        };
        storage_ptr.copy_from_slice(storage_source);

        let code_offset = storage_offset + STORAGE_LENGTH;
        let code_ptr = &mut data[code_offset..][..code.len()];
        code_ptr.copy_from_slice(code);
    }

    Account {
        lamports: rent.minimum_balance(data.len()),
        data,
        owner: *program_id,
        executable: false,
        rent_epoch: 0,
    }
}

fn create_legacy_ether_account(
    program_id: &Pubkey,
    rent: &Rent,
    address: Address,
    balance: U256,
    trx_count: u64,
) -> Account {
    let storage = [[0u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT];
    create_legacy_ether_contract(
        program_id,
        rent,
        address,
        balance,
        trx_count,
        0u32,
        &[],
        &storage,
    )
}

struct ActualStorage {
    index: U256,
    values: &'static [(u8, [u8; 32])],
}

struct LegacyStorage {
    generation: u32,
    index: U256,
    values: &'static [(u8, [u8; 32])],
}

impl ActualStorage {
    pub fn account_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
        address: Address,
    ) -> (Pubkey, Account) {
        let (contract, _) = address.find_solana_address(program_id);
        let cell_address = StorageCellAddress::new(program_id, &contract, &self.index);
        let cell_pubkey = *cell_address.pubkey();
        let mut account_data = AccountData::new(cell_pubkey);
        account_data.assign(*program_id).unwrap();
        account_data.expand(StorageCell::required_account_size(self.values.len()));
        account_data.lamports = rent.minimum_balance(account_data.get_length());
        let mut storage =
            StorageCell::initialize(account_data.into_account_info(), program_id).unwrap();
        for (cell, (index, value)) in storage.cells_mut().iter_mut().zip(self.values.iter()) {
            cell.subindex = *index;
            cell.value.copy_from_slice(value);
        }
        (
            cell_pubkey,
            Account {
                lamports: rent.minimum_balance(account_data.get_length()),
                data: account_data.data().to_vec(),
                owner: *program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
    }
}

impl LegacyStorage {
    pub const fn required_account_size(count: usize) -> usize {
        1 + LegacyStorageData::SIZE + std::mem::size_of::<(u8, [u8; 32])>() * count
    }
    pub fn account_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
        address: Address,
    ) -> (Pubkey, Account) {
        let (contract, _) = address.find_solana_address(program_id);
        let cell_address = StorageCellAddress::new(program_id, &contract, &self.index);
        let cell_pubkey = *cell_address.pubkey();
        let mut data = vec![0u8; Self::required_account_size(self.values.len())];

        let data_ref = arrayref::array_mut_ref![data, 0, 1 + LegacyStorageData::SIZE];
        let (tag_ptr, address_ptr, generation_ptr, index_ptr) =
            arrayref::mut_array_refs![data_ref, 1, 20, 4, 32];

        *tag_ptr = LegacyStorageData::TAG.to_le_bytes();
        *address_ptr = *address.as_bytes();
        *generation_ptr = self.generation.to_le_bytes();
        *index_ptr = self.index.to_le_bytes();

        let storage = unsafe {
            let data = &mut data[1 + LegacyStorageData::SIZE..];
            let ptr = data.as_mut_ptr().cast::<(u8, [u8; 32])>();
            std::slice::from_raw_parts_mut(ptr, self.values.len())
        };
        storage.copy_from_slice(self.values);

        let account = Account {
            lamports: rent.minimum_balance(data.len()),
            data,
            owner: *program_id,
            executable: false,
            rent_epoch: 0,
        };

        (cell_pubkey, account)
    }
}

struct LegacyAccount {
    pub address: Address,
    pub balance: U256,
    pub nonce: u64,
}

impl LegacyAccount {
    pub fn account_with_pubkey(&self, program_id: &Pubkey, rent: &Rent) -> (Pubkey, Account) {
        (
            self.address.find_solana_address(program_id).0,
            create_legacy_ether_account(program_id, rent, self.address, self.balance, self.nonce),
        )
    }
}
struct LegacyContract {
    pub address: Address,
    pub balance: U256,
    pub nonce: u64,
    pub generation: u32,
    pub code: &'static [u8],
    pub storage: [[u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],

    pub legacy_storage: LegacyStorage,
    pub outdate_storage: LegacyStorage,
}

impl LegacyContract {
    fn account_with_pubkey(&self, program_id: &Pubkey, rent: &Rent) -> (Pubkey, Account) {
        (
            self.address.find_solana_address(program_id).0,
            create_legacy_ether_contract(
                program_id,
                rent,
                self.address,
                self.balance,
                self.nonce,
                self.generation,
                self.code,
                &self.storage,
            ),
        )
    }

    pub fn legacy_storage_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
    ) -> (Pubkey, Account) {
        self.legacy_storage
            .account_with_pubkey(program_id, rent, self.address)
    }

    pub fn outdate_storage_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
    ) -> (Pubkey, Account) {
        self.outdate_storage
            .account_with_pubkey(program_id, rent, self.address)
    }
}

struct ActualBalance {
    pub address: Address,
    pub chain_id: u64,
    pub balance: U256,
    pub nonce: u64,
}

impl ActualBalance {
    pub fn account_with_pubkey(&self, program_id: &Pubkey, rent: &Rent) -> (Pubkey, Account) {
        let (pubkey, _) = self.address.find_balance_address(program_id, self.chain_id);
        let mut account_data = AccountData::new(pubkey);
        account_data.assign(*program_id).unwrap();
        account_data.expand(BalanceAccount::required_account_size());
        account_data.lamports = rent.minimum_balance(account_data.get_length());

        let mut balance = BalanceAccount::initialize(
            account_data.into_account_info(),
            program_id,
            self.address,
            self.chain_id,
        )
        .unwrap();
        balance.mint(self.balance).unwrap();
        balance.increment_nonce_by(self.nonce).unwrap();

        (
            pubkey,
            Account {
                lamports: rent.minimum_balance(account_data.get_length()),
                data: account_data.data().to_vec(),
                owner: *program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
    }
}

struct ActualContract {
    pub address: Address,
    pub chain_id: u64,
    pub generation: u32,
    pub code: &'static [u8],
    pub storage: [[u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],

    pub actual_storage: ActualStorage,
    pub legacy_storage: LegacyStorage,
    pub outdate_storage: LegacyStorage,
}

impl ActualContract {
    pub fn account_with_pubkey(&self, program_id: &Pubkey, rent: &Rent) -> (Pubkey, Account) {
        let (pubkey, _) = self.address.find_solana_address(program_id);
        let mut account_data = AccountData::new(pubkey);
        account_data.assign(*program_id).unwrap();
        account_data.expand(ContractAccount::required_account_size(self.code));
        account_data.lamports = rent.minimum_balance(account_data.get_length());

        let mut contract = ContractAccount::initialize(
            account_data.into_account_info(),
            program_id,
            self.address,
            self.chain_id,
            self.generation,
            self.code,
        )
        .unwrap();
        contract.set_storage_multiple_values(0, &self.storage);

        (
            pubkey,
            Account {
                lamports: rent.minimum_balance(account_data.get_length()),
                data: account_data.data().to_vec(),
                owner: *program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    pub fn actual_storage_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
    ) -> (Pubkey, Account) {
        self.actual_storage
            .account_with_pubkey(program_id, rent, self.address)
    }

    pub fn legacy_storage_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
    ) -> (Pubkey, Account) {
        self.legacy_storage
            .account_with_pubkey(program_id, rent, self.address)
    }

    pub fn outdate_storage_with_pubkey(
        &self,
        program_id: &Pubkey,
        rent: &Rent,
    ) -> (Pubkey, Account) {
        self.outdate_storage
            .account_with_pubkey(program_id, rent, self.address)
    }
}

const LEGACY_CHAIN_ID: u64 = 1;
const EXTRA_CHAIN_ID: u64 = 2;
const MISSING_ADDRESS: Address = Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24800"));

const MISSING_STORAGE_INDEX: U256 = U256::new(256u128);
const ACTUAL_STORAGE_INDEX: U256 = U256::new(2 * 256u128);
const LEGACY_STORAGE_INDEX: U256 = U256::new(3 * 256u128);
const OUTDATE_STORAGE_INDEX: U256 = U256::new(4 * 256u128);

const ACTUAL_BALANCE: ActualBalance = ActualBalance {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24810")),
    chain_id: LEGACY_CHAIN_ID,
    balance: U256::new(1513),
    nonce: 41,
};

const ACTUAL_BALANCE2: ActualBalance = ActualBalance {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24811")),
    chain_id: EXTRA_CHAIN_ID,
    balance: U256::new(5134),
    nonce: 14,
};

const ACTUAL_CONTRACT: ActualContract = ActualContract {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24c11")),
    chain_id: LEGACY_CHAIN_ID,
    generation: 4,
    code: &[0x03, 0x04, 0x05],
    storage: [[14u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],
    actual_storage: ActualStorage {
        index: ACTUAL_STORAGE_INDEX,
        values: &[(0u8, [64u8; 32])],
    },
    legacy_storage: LegacyStorage {
        generation: 4,
        index: LEGACY_STORAGE_INDEX,
        values: &[(0u8, [54u8; 32])],
    },
    outdate_storage: LegacyStorage {
        generation: 3,
        index: OUTDATE_STORAGE_INDEX,
        values: &[(0u8, [34u8; 32])],
    },
};

const ACTUAL_SUICIDE: ActualContract = ActualContract {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24d10")),
    chain_id: LEGACY_CHAIN_ID,
    generation: 12,
    code: &[],
    storage: [[0u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT], // It's matter that suicide contract doesn't contains any values in storage!
    actual_storage: ActualStorage {
        index: U256::ZERO,
        values: &[],
    },
    legacy_storage: LegacyStorage {
        generation: 0,
        index: U256::ZERO,
        values: &[],
    },
    outdate_storage: LegacyStorage {
        generation: 11,
        index: LEGACY_STORAGE_INDEX,
        values: &[(0u8, [13u8; 32])],
    },
};

const LEGACY_ACCOUNT: LegacyAccount = LegacyAccount {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24820")),
    balance: U256::new(10234),
    nonce: 123,
};

const LEGACY_CONTRACT: LegacyContract = LegacyContract {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24c21")),
    balance: U256::new(6153),
    nonce: 1,
    generation: 3,
    code: &[0x01, 0x02, 0x03],
    storage: [[0u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],

    legacy_storage: LegacyStorage {
        generation: 3,
        index: LEGACY_STORAGE_INDEX,
        values: &[(0u8, [23u8; 32])],
    },
    outdate_storage: LegacyStorage {
        generation: 2,
        index: OUTDATE_STORAGE_INDEX,
        values: &[(0u8, [43u8; 32])],
    },
};

const LEGACY_CONTRACT_NO_BALANCE: LegacyContract = LegacyContract {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24c20")),
    balance: U256::ZERO,
    nonce: 0,
    generation: 2,
    code: &[0x01, 0x02, 0x03, 0x04],
    storage: [[53u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],
    legacy_storage: LegacyStorage {
        generation: 0,
        index: U256::ZERO,
        values: &[],
    },
    outdate_storage: LegacyStorage {
        generation: 1,
        index: U256::ZERO,
        values: &[],
    },
};

const LEGACY_SUICIDE: LegacyContract = LegacyContract {
    address: Address(hex!("7a250d5630b4cf539739df2c5dacb4c659f24d21")),
    balance: U256::new(41234),
    nonce: 413,
    generation: 5,
    code: &[],
    storage: [[42u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT],

    legacy_storage: LegacyStorage {
        generation: 413,
        index: LEGACY_STORAGE_INDEX,
        values: &[(0u8, [65u8; 32])],
    },
    outdate_storage: LegacyStorage {
        generation: 412,
        index: OUTDATE_STORAGE_INDEX,
        values: &[(0u8, [76u8; 32])],
    },
};

struct Fixture {
    program_id: Pubkey,
    chains: Vec<ChainInfo>,
    rent: Rent,
    mock_rpc: mock_rpc_client::MockRpcClient,
    block_overrides: Option<BlockOverrides>,
    state_overrides: Option<HashMap<Address, AccountOverride>>,
    solana_overrides: Option<SolanaOverrides>,
}

impl Fixture {
    pub fn new() -> Self {
        let rent = Rent::default();
        let program_id = Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
        let accounts = vec![
            (
                Pubkey::from_str("SysvarRent111111111111111111111111111111111").unwrap(),
                Account {
                    lamports: 1_009_200,
                    data: bincode::serialize(&rent).unwrap(),
                    owner: Pubkey::from_str("Sysvar1111111111111111111111111111111111111").unwrap(),
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            ACTUAL_BALANCE.account_with_pubkey(&program_id, &rent),
            ACTUAL_BALANCE2.account_with_pubkey(&program_id, &rent),
            LEGACY_ACCOUNT.account_with_pubkey(&program_id, &rent),
            ACTUAL_CONTRACT.account_with_pubkey(&program_id, &rent),
            ACTUAL_CONTRACT.actual_storage_with_pubkey(&program_id, &rent),
            ACTUAL_CONTRACT.legacy_storage_with_pubkey(&program_id, &rent),
            ACTUAL_CONTRACT.outdate_storage_with_pubkey(&program_id, &rent),
            ACTUAL_SUICIDE.account_with_pubkey(&program_id, &rent),
            ACTUAL_SUICIDE.outdate_storage_with_pubkey(&program_id, &rent),
            LEGACY_CONTRACT.account_with_pubkey(&program_id, &rent),
            LEGACY_CONTRACT.legacy_storage_with_pubkey(&program_id, &rent),
            LEGACY_CONTRACT.outdate_storage_with_pubkey(&program_id, &rent),
            LEGACY_CONTRACT_NO_BALANCE.account_with_pubkey(&program_id, &rent),
            LEGACY_SUICIDE.account_with_pubkey(&program_id, &rent),
            LEGACY_SUICIDE.outdate_storage_with_pubkey(&program_id, &rent),
        ];

        let rpc_client = mock_rpc_client::MockRpcClient::new(&accounts);

        Self {
            program_id,
            chains: vec![
                ChainInfo {
                    id: LEGACY_CHAIN_ID,
                    name: "neon".to_string(),
                    token: Pubkey::new_unique(),
                },
                ChainInfo {
                    id: EXTRA_CHAIN_ID,
                    name: "usdt".to_string(),
                    token: Pubkey::new_unique(),
                },
            ],
            rent,
            mock_rpc: rpc_client,
            block_overrides: None,
            state_overrides: None,
            solana_overrides: None,
        }
    }

    pub async fn build_account_storage_with_chain_id(
        &self,
        tx_chain_id: Option<u64>,
    ) -> EmulatorAccountStorage<'_, mock_rpc_client::MockRpcClient> {
        EmulatorAccountStorage::new(
            &self.mock_rpc,
            self.program_id,
            Some(self.chains.clone()),
            self.block_overrides.clone(),
            self.state_overrides.clone(),
            self.solana_overrides.clone(),
            tx_chain_id,
        )
        .await
        .unwrap()
    }

    pub async fn build_account_storage(
        &self,
    ) -> EmulatorAccountStorage<'_, mock_rpc_client::MockRpcClient> {
        EmulatorAccountStorage::new(
            &self.mock_rpc,
            self.program_id,
            Some(self.chains.clone()),
            self.block_overrides.clone(),
            self.state_overrides.clone(),
            self.solana_overrides.clone(),
            None,
        )
        .await
        .unwrap()
    }

    pub fn balance_pubkey(&self, address: Address, chain_id: u64) -> Pubkey {
        address.find_balance_address(&self.program_id, chain_id).0
    }

    pub fn legacy_pubkey(&self, address: Address) -> Pubkey {
        address.find_solana_address(&self.program_id).0
    }

    pub fn contract_pubkey(&self, address: Address) -> Pubkey {
        address.find_solana_address(&self.program_id).0
    }

    pub fn storage_pubkey(&self, address: Address, index: U256) -> Pubkey {
        if index < U256::new(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u128) {
            self.contract_pubkey(address)
        } else {
            let index = index & !U256::new(0xFF);
            let base = self.contract_pubkey(address);
            let cell_address = StorageCellAddress::new(&self.program_id, &base, &index);
            *cell_address.pubkey()
        }
    }

    pub fn storage_rent(&self, count: usize) -> u64 {
        self.rent
            .minimum_balance(StorageCell::required_account_size(count))
    }

    pub fn legacy_storage_rent(&self, count: usize) -> u64 {
        self.rent
            .minimum_balance(LegacyStorage::required_account_size(count))
    }

    pub fn balance_rent(&self) -> u64 {
        self.rent
            .minimum_balance(BalanceAccount::required_account_size())
    }

    pub fn legacy_rent(&self, code_len: Option<usize>) -> u64 {
        let data_length = code_len.map_or(1 + LegacyEtherData::SIZE, |len| {
            1 + LegacyEtherData::SIZE + 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT + len
        });
        self.rent.minimum_balance(data_length)
    }

    pub fn contract_rent(&self, code: &[u8]) -> u64 {
        self.rent
            .minimum_balance(ContractAccount::required_account_size(code))
    }
}

impl<'rpc, T: Rpc> EmulatorAccountStorage<'rpc, T> {
    pub fn verify_used_accounts(&self, expected: &[(Pubkey, bool, bool)]) {
        let mut expected = expected.to_vec();
        expected.sort_by_key(|(k, _, _)| *k);
        let mut actual = self
            .used_accounts()
            .iter()
            .map(|v| (v.pubkey, v.is_writable, v.is_legacy))
            .collect::<Vec<_>>();
        actual.sort_by_key(|(k, _, _)| *k);
        assert_eq!(actual, expected);
    }

    pub fn verify_upgrade_rent(&self, added_rent: u64, removed_rent: u64) {
        assert_eq!(
            self.get_upgrade_rent().unwrap(),
            added_rent.saturating_sub(removed_rent)
        );
    }

    pub fn verify_regular_rent(&self, added_rent: u64, removed_rent: u64) {
        assert_eq!(
            self.get_regular_rent().unwrap(),
            added_rent.saturating_sub(removed_rent)
        );
    }
}

#[tokio::test]
async fn test_read_balance_missing_account() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    assert_eq!(
        storage.balance(MISSING_ADDRESS, LEGACY_CHAIN_ID).await,
        U256::ZERO
    );
    assert_eq!(storage.nonce(MISSING_ADDRESS, LEGACY_CHAIN_ID).await, 0);

    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(MISSING_ADDRESS, LEGACY_CHAIN_ID),
            false,
            false,
        ),
        (fixture.legacy_pubkey(MISSING_ADDRESS), false, false),
    ]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_balance_missing_account_extra_chain() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    assert_eq!(
        storage.balance(MISSING_ADDRESS, EXTRA_CHAIN_ID).await,
        U256::ZERO
    );
    assert_eq!(storage.nonce(MISSING_ADDRESS, EXTRA_CHAIN_ID).await, 0);

    storage.verify_used_accounts(&[(
        fixture.balance_pubkey(MISSING_ADDRESS, EXTRA_CHAIN_ID),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_balance_actual_account() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let acc = &ACTUAL_BALANCE;
    assert_eq!(
        storage.balance(acc.address, acc.chain_id).await,
        acc.balance
    );
    assert_eq!(storage.nonce(acc.address, acc.chain_id).await, acc.nonce);

    storage.verify_used_accounts(&[(
        fixture.balance_pubkey(acc.address, acc.chain_id),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_balance_actual_account_extra_chain() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let acc = &ACTUAL_BALANCE2;
    assert_eq!(acc.chain_id, EXTRA_CHAIN_ID);
    assert_eq!(
        storage.balance(acc.address, acc.chain_id).await,
        acc.balance
    );
    assert_eq!(storage.nonce(acc.address, acc.chain_id).await, acc.nonce);

    storage.verify_used_accounts(&[(
        fixture.balance_pubkey(acc.address, acc.chain_id),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_balance_legacy_account() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let acc = &LEGACY_ACCOUNT;
    assert_eq!(
        storage.balance(acc.address, LEGACY_CHAIN_ID).await,
        acc.balance
    );
    assert_eq!(storage.nonce(acc.address, LEGACY_CHAIN_ID).await, acc.nonce);

    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(acc.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.legacy_pubkey(acc.address), true, true),
    ]);
    storage.verify_upgrade_rent(fixture.balance_rent(), fixture.legacy_rent(None));
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_modify_actual_and_missing_account() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let from = &ACTUAL_BALANCE;
    let amount = U256::new(10);
    assert_eq!(from.chain_id, LEGACY_CHAIN_ID);
    assert!(storage
        .transfer(from.address, MISSING_ADDRESS, from.chain_id, amount)
        .await
        .is_ok());

    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(from.address, from.chain_id),
            true,
            false,
        ),
        (
            fixture.balance_pubkey(MISSING_ADDRESS, LEGACY_CHAIN_ID),
            true,
            false,
        ),
        (fixture.legacy_pubkey(MISSING_ADDRESS), false, false),
    ]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.balance_rent(), 0);

    assert_eq!(
        storage.balance(from.address, from.chain_id).await,
        from.balance - amount
    );
    assert_eq!(
        storage.balance(MISSING_ADDRESS, LEGACY_CHAIN_ID).await,
        amount
    );
}

#[tokio::test]
async fn test_modify_actual_and_missing_account_extra_chain() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let from = &ACTUAL_BALANCE2;
    let amount = U256::new(11);
    assert_eq!(from.chain_id, EXTRA_CHAIN_ID);
    assert!(storage
        .transfer(from.address, MISSING_ADDRESS, from.chain_id, amount)
        .await
        .is_ok());

    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(from.address, from.chain_id),
            true,
            false,
        ),
        (
            fixture.balance_pubkey(MISSING_ADDRESS, from.chain_id),
            true,
            false,
        ),
    ]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.balance_rent(), 0);

    assert_eq!(
        storage.balance(from.address, from.chain_id).await,
        from.balance - amount
    );
    assert_eq!(
        storage.balance(MISSING_ADDRESS, from.chain_id).await,
        amount
    );
}

#[tokio::test]
async fn test_modify_actual_and_legacy_account() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let from = &ACTUAL_BALANCE;
    let to = &LEGACY_ACCOUNT;
    let amount = U256::new(10);
    assert_eq!(from.chain_id, LEGACY_CHAIN_ID);
    assert!(storage
        .transfer(from.address, to.address, from.chain_id, amount)
        .await
        .is_ok());

    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(from.address, from.chain_id),
            true,
            false,
        ),
        (
            fixture.balance_pubkey(to.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.legacy_pubkey(to.address), true, true),
    ]);
    storage.verify_upgrade_rent(fixture.balance_rent(), fixture.legacy_rent(None));
    storage.verify_regular_rent(0, 0);

    assert_eq!(
        storage.balance(from.address, from.chain_id).await,
        from.balance - amount
    );
    assert_eq!(
        storage.balance(to.address, LEGACY_CHAIN_ID).await,
        to.balance + amount
    );
}

#[tokio::test]
async fn test_read_missing_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    assert_eq!(*storage.code(MISSING_ADDRESS).await, [0u8; 0]);
    assert_eq!(
        storage.storage(MISSING_ADDRESS, U256::ZERO).await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(fixture.contract_pubkey(MISSING_ADDRESS), false, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);

    assert_eq!(
        storage
            .storage(
                MISSING_ADDRESS,
                U256::new(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u128)
            )
            .await,
        [0u8; 32]
    );
}

#[tokio::test]
async fn test_read_legacy_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    assert_eq!(
        *storage.code(LEGACY_CONTRACT.address).await,
        *LEGACY_CONTRACT.code
    );
    assert_eq!(
        storage.storage(LEGACY_CONTRACT.address, U256::ZERO).await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(LEGACY_CONTRACT.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.contract_pubkey(LEGACY_CONTRACT.address), true, true),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(LEGACY_CONTRACT.code),
        fixture.legacy_rent(Some(LEGACY_CONTRACT.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_legacy_contract_no_balance() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_CONTRACT_NO_BALANCE;
    assert_eq!(*storage.code(contract.address).await, *contract.code);
    assert_eq!(
        storage.storage(contract.address, U256::ZERO).await,
        [53u8; 32]
    );
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            false,
            true,
        ),
        (fixture.contract_pubkey(contract.address), true, true),
    ]);
    storage.verify_upgrade_rent(
        fixture.contract_rent(contract.code),
        fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_actual_suicide_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_SUICIDE;
    assert_eq!(*storage.code(contract.address).await, [0u8; 0]);
    assert_eq!(
        storage.storage(contract.address, U256::ZERO).await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), false, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_legacy_suicide_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_SUICIDE;
    assert_eq!(*storage.code(contract.address).await, [0u8; 0]);
    assert_eq!(
        storage.storage(contract.address, U256::ZERO).await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.contract_pubkey(contract.address), true, true),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code),
        fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_deploy_at_missing_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("14643165").to_vec();
    assert!(storage
        .set_code(MISSING_ADDRESS, LEGACY_CHAIN_ID, code.clone())
        .await
        .is_ok());
    storage.verify_used_accounts(&[(fixture.contract_pubkey(MISSING_ADDRESS), true, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.contract_rent(&code), 0);
}

#[tokio::test]
async fn test_deploy_at_actual_balance() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("14643165").to_vec();
    let acc = &ACTUAL_BALANCE;
    assert!(storage
        .set_code(acc.address, LEGACY_CHAIN_ID, code.clone())
        .await
        .is_ok());
    storage.verify_used_accounts(&[(fixture.contract_pubkey(acc.address), true, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.contract_rent(&code), 0);
}

#[tokio::test]
async fn test_deploy_at_actual_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("62345987").to_vec();
    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .set_code(contract.address, LEGACY_CHAIN_ID, code)
            .await
            .unwrap_err()
            .to_string(),
        EvmLoaderError::AccountAlreadyInitialized(fixture.contract_pubkey(contract.address))
            .to_string()
    );
    storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), false, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_deploy_at_legacy_account() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("37455846").to_vec();
    let contract = &LEGACY_ACCOUNT;
    assert!(storage
        .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
        .await
        .is_ok());
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.contract_pubkey(contract.address), true, true),
    ]);
    storage.verify_upgrade_rent(fixture.balance_rent(), fixture.legacy_rent(None));
    storage.verify_regular_rent(fixture.contract_rent(&code), 0);
}

#[tokio::test]
async fn test_deploy_at_legacy_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("13412971").to_vec();
    let contract = &LEGACY_CONTRACT;
    assert_eq!(
        storage
            .set_code(contract.address, LEGACY_CHAIN_ID, code)
            .await
            .unwrap_err()
            .to_string(),
        EvmLoaderError::AccountAlreadyInitialized(fixture.contract_pubkey(contract.address))
            .to_string()
    );
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.contract_pubkey(contract.address), true, true),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code),
        fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_deploy_at_actual_suicide() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("13412971").to_vec();
    let contract = &ACTUAL_SUICIDE;
    // TODO: Should we deploy new contract by the previous address?
    assert!(storage
        .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
        .await
        .is_ok(),);
    storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), true, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(
        fixture.contract_rent(&code),
        fixture.contract_rent(contract.code),
    );
}

#[tokio::test]
async fn test_deploy_at_legacy_suicide() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let code = hex!("13412971").to_vec();
    let contract = &LEGACY_SUICIDE;
    // TODO: Should we deploy new contract by the previous address?
    assert!(storage
        .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
        .await
        .is_ok(),);
    storage.verify_used_accounts(&[
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (fixture.contract_pubkey(contract.address), true, true),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code),
        fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(
        fixture.contract_rent(&code),
        fixture.contract_rent(contract.code),
    );
}

#[tokio::test]
async fn test_read_missing_storage_for_missing_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    assert_eq!(
        storage
            .storage(MISSING_ADDRESS, MISSING_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(MISSING_ADDRESS, MISSING_STORAGE_INDEX),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_missing_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, MISSING_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, MISSING_STORAGE_INDEX),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_actual_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, ACTUAL_STORAGE_INDEX)
            .await,
        contract.actual_storage.values[0].1
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, ACTUAL_STORAGE_INDEX),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_modify_new_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, ACTUAL_STORAGE_INDEX + 1)
            .await,
        [0u8; 32]
    );
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);

    let new_value = [0x01u8; 32];
    assert!(storage
        .set_storage(contract.address, ACTUAL_STORAGE_INDEX + 1, new_value)
        .await
        .is_ok());
    assert_eq!(
        storage
            .storage(contract.address, ACTUAL_STORAGE_INDEX + 1)
            .await,
        new_value
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, ACTUAL_STORAGE_INDEX),
        true,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.storage_rent(2), fixture.storage_rent(1));
}

#[tokio::test]
async fn test_modify_missing_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    let new_value = [0x02u8; 32];
    assert!(storage
        .set_storage(contract.address, MISSING_STORAGE_INDEX, new_value)
        .await
        .is_ok());
    assert_eq!(
        storage
            .storage(contract.address, MISSING_STORAGE_INDEX)
            .await,
        new_value
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, MISSING_STORAGE_INDEX),
        true,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(fixture.storage_rent(1), 0);
}

#[tokio::test]
async fn test_modify_internal_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let mut storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    let new_value = [0x03u8; 32];
    let index = U256::new(0);
    assert!(storage
        .set_storage(contract.address, index, new_value)
        .await
        .is_ok());
    assert_eq!(storage.storage(contract.address, index).await, new_value);
    storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), true, false)]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_legacy_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, LEGACY_STORAGE_INDEX)
            .await,
        contract.legacy_storage.values[0].1
    );
    storage.verify_used_accounts(&[
        (fixture.contract_pubkey(contract.address), false, true),
        (
            fixture.storage_pubkey(contract.address, LEGACY_STORAGE_INDEX),
            true,
            true,
        ),
    ]);
    storage.verify_upgrade_rent(fixture.storage_rent(1), fixture.legacy_storage_rent(1));
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_outdate_storage_for_actual_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &ACTUAL_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, OUTDATE_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[
        (fixture.contract_pubkey(contract.address), false, true),
        (
            fixture.storage_pubkey(contract.address, OUTDATE_STORAGE_INDEX),
            true,
            true,
        ),
    ]);
    storage.verify_upgrade_rent(0, fixture.legacy_storage_rent(1));
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_missing_storage_for_legacy_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, MISSING_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, MISSING_STORAGE_INDEX),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_legacy_storage_for_legacy_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, LEGACY_STORAGE_INDEX)
            .await,
        contract.legacy_storage.values[0].1
    );
    storage.verify_used_accounts(&[
        (fixture.contract_pubkey(contract.address), true, true),
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (
            fixture.storage_pubkey(contract.address, LEGACY_STORAGE_INDEX),
            true,
            true,
        ),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code) + fixture.storage_rent(1),
        fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_outdate_storage_for_legacy_contract() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_CONTRACT;
    assert_eq!(
        storage
            .storage(contract.address, OUTDATE_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[
        (fixture.contract_pubkey(contract.address), true, true),
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (
            fixture.storage_pubkey(contract.address, OUTDATE_STORAGE_INDEX),
            true,
            true,
        ),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code),
        fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_missing_storage_for_legacy_suicide() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_SUICIDE;
    assert_eq!(
        storage
            .storage(contract.address, MISSING_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[(
        fixture.storage_pubkey(contract.address, MISSING_STORAGE_INDEX),
        false,
        false,
    )]);
    storage.verify_upgrade_rent(0, 0);
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_read_outdate_storage_for_legacy_suicide() {
    let fixture = Fixture::new();
    let storage = fixture.build_account_storage().await;

    let contract = &LEGACY_SUICIDE;
    assert_eq!(
        storage
            .storage(contract.address, OUTDATE_STORAGE_INDEX)
            .await,
        [0u8; 32]
    );
    storage.verify_used_accounts(&[
        (fixture.contract_pubkey(contract.address), true, true),
        (
            fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
            true,
            true,
        ),
        (
            fixture.storage_pubkey(contract.address, OUTDATE_STORAGE_INDEX),
            true,
            true,
        ),
    ]);
    storage.verify_upgrade_rent(
        fixture.balance_rent() + fixture.contract_rent(contract.code),
        fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
    );
    storage.verify_regular_rent(0, 0);
}

#[tokio::test]
async fn test_state_overrides_nonce_and_balance() {
    let expected_nonce = 17;
    let expected_balance = U256::MAX;

    let overriden_state = AccountOverrides::from([
        (
            ACTUAL_BALANCE.address,
            AccountOverride {
                nonce: Some(expected_nonce),
                balance: Some(expected_balance),
                ..Default::default()
            },
        ),
        (
            ACTUAL_BALANCE2.address,
            AccountOverride {
                nonce: Some(expected_nonce),
                ..Default::default()
            },
        ),
    ]);

    // Checking override for another acount and chain where we expect only
    // nonce overriden.
    assert_eq!(
        get_overriden_nonce_and_balance(
            ACTUAL_BALANCE2.address,
            EXTRA_CHAIN_ID,
            EXTRA_CHAIN_ID,
            Some(overriden_state.clone())
        )
        .await,
        (expected_nonce, ACTUAL_BALANCE2.balance)
    );

    // Checking override for another for first account for both
    // balance and nonce.
    assert_eq!(
        get_overriden_nonce_and_balance(
            ACTUAL_BALANCE.address,
            LEGACY_CHAIN_ID,
            LEGACY_CHAIN_ID,
            Some(overriden_state.clone())
        )
        .await,
        (expected_nonce, expected_balance)
    );

    // Override for different chain id.
    assert_ne!(expected_nonce, ACTUAL_BALANCE.nonce);
    assert_eq!(
        get_overriden_nonce_and_balance(
            ACTUAL_BALANCE.address,
            EXTRA_CHAIN_ID,
            LEGACY_CHAIN_ID,
            Some(overriden_state.clone())
        )
        .await,
        (ACTUAL_BALANCE.nonce, ACTUAL_BALANCE.balance)
    );

    // Do not override if all items are None.
    assert_eq!(
        get_overriden_nonce_and_balance(
            ACTUAL_BALANCE.address,
            LEGACY_CHAIN_ID,
            LEGACY_CHAIN_ID,
            Some(AccountOverrides::from([
                (ACTUAL_BALANCE.address, AccountOverride::default()),
                (ACTUAL_BALANCE2.address, AccountOverride::default())
            ]))
        )
        .await,
        (ACTUAL_BALANCE.nonce, ACTUAL_BALANCE.balance)
    );
}

#[tokio::test]
async fn test_storage_with_accounts_and_override() {
    let expected_nonce = 17;
    let expected_balance = U256::MAX;

    let rent = Rent::default();
    let program_id = Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
    let account_tuple = ACTUAL_BALANCE.account_with_pubkey(&program_id, &rent);
    let accounts_for_rpc = vec![
        (solana_sdk::sysvar::rent::id(), account_tuple.1.clone()),
        account_tuple.clone(),
    ];
    let rpc_client = mock_rpc_client::MockRpcClient::new(&accounts_for_rpc);
    let accounts_for_storage: Vec<Pubkey> = vec![account_tuple.0];
    let storage = EmulatorAccountStorage::with_accounts(
        &rpc_client,
        program_id,
        &accounts_for_storage,
        vec![ChainInfo {
            id: LEGACY_CHAIN_ID,
            name: "neon".to_string(),
            token: Pubkey::new_unique(),
        }]
        .into(),
        None,
        Some(AccountOverrides::from([(
            ACTUAL_BALANCE.address,
            AccountOverride {
                nonce: Some(expected_nonce),
                balance: Some(expected_balance),
                ..Default::default()
            },
        )])),
        None,
        Some(LEGACY_CHAIN_ID),
    )
    .await
    .expect("Failed to create storage");
    assert_eq!(
        get_balance_account_info(&storage, |account: &BalanceAccount| account.nonce())
            .await
            .expect("Failed to read nonce"),
        expected_nonce
    );
    assert_eq!(
        get_balance_account_info(&storage, |account: &BalanceAccount| account.balance())
            .await
            .expect("Failed to read balance"),
        expected_balance
    );
}

#[tokio::test]
async fn test_storage_new_from_other_and_override() {
    let expected_nonce = 17;
    let expected_balance = U256::MAX;

    let rent = Rent::default();
    let program_id = Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
    let account_tuple = ACTUAL_BALANCE.account_with_pubkey(&program_id, &rent);
    let accounts_for_rpc = vec![
        (solana_sdk::sysvar::rent::id(), account_tuple.1.clone()),
        account_tuple.clone(),
    ];
    let rpc_client = mock_rpc_client::MockRpcClient::new(&accounts_for_rpc);
    let accounts_for_storage: Vec<Pubkey> = vec![account_tuple.0];
    let storage = EmulatorAccountStorage::with_accounts(
        &rpc_client,
        program_id,
        &accounts_for_storage,
        vec![ChainInfo {
            id: LEGACY_CHAIN_ID,
            name: "neon".to_string(),
            token: Pubkey::new_unique(),
        }]
        .into(),
        None,
        Some(AccountOverrides::from([(
            ACTUAL_BALANCE.address,
            AccountOverride {
                nonce: Some(expected_nonce),
                balance: Some(expected_balance),
                ..Default::default()
            },
        )])),
        None,
        Some(LEGACY_CHAIN_ID),
    )
    .await
    .expect("Failed to create storage");

    let other_storage =
        EmulatorAccountStorage::new_from_other(&storage, 0, 0, Some(LEGACY_CHAIN_ID))
            .await
            .expect("Failed to create a copy of storage");
    assert_eq!(
        get_balance_account_info(&other_storage, |account: &BalanceAccount| account.nonce())
            .await
            .expect("Failed to read nonce"),
        expected_nonce
    );
    assert_eq!(
        get_balance_account_info(&other_storage, |account: &BalanceAccount| account.balance())
            .await
            .expect("Failed to read balance"),
        expected_balance
    );
}
