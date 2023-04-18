use std::{cell::RefCell, collections::HashMap, convert::TryInto, rc::Rc, str::FromStr};

use ethnum::U256;
use evm_loader::account::ether_contract;
use evm_loader::account_storage::{generate_fake_block_hash, AccountOperation, AccountsOperations};
use evm_loader::{
    account::{
        ether_storage::EthereumStorageAddress, EthereumAccount, EthereumStorage,
        ACCOUNT_SEED_VERSION,
    },
    account_storage::AccountStorage,
    config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
    evm::is_precompile_address,
    executor::{Action, OwnedAccountInfo, OwnedAccountInfoPartial},
    gasometer::LAMPORTS_PER_SIGNATURE,
    types::Address,
};
use log::{debug, info, trace, warn};
use solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE;
use solana_sdk::{
    account::Account,
    account_info::AccountInfo,
    pubkey,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::{recent_blockhashes, slot_hashes, Sysvar},
};

use crate::rpc::Rpc;

const FAKE_OPERATOR: Pubkey = pubkey!("neonoperator1111111111111111111111111111111");

fn serde_pubkey_bs58<S>(value: &Pubkey, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bs58 = bs58::encode(value).into_string();
    s.serialize_str(&bs58)
}

#[allow(unused)]
fn deserialize_pubkey_from_str<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    struct StringVisitor;
    impl<'de> serde::de::Visitor<'de> for StringVisitor {
        type Value = Pubkey;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string containing json data")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Pubkey::from_str(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_any(StringVisitor)
}

#[derive(serde::Serialize, Clone)]
pub struct NeonAccount {
    address: Address,
    #[serde(serialize_with = "serde_pubkey_bs58")]
    #[serde(deserialize_with = "deserialize_pubkey_from_str")]
    account: Pubkey,
    writable: bool,
    new: bool,
    size: usize,
    size_current: usize,
    additional_resize_steps: usize,
    #[serde(skip)]
    data: Option<Account>,
}

impl NeonAccount {
    fn new(address: Address, pubkey: Pubkey, account: Option<Account>, writable: bool) -> Self {
        if let Some(account) = account {
            trace!("Account found {}", address);

            Self {
                address,
                account: pubkey,
                writable,
                new: false,
                size: account.data.len(),
                size_current: account.data.len(),
                additional_resize_steps: 0,
                data: Some(account),
            }
        } else {
            trace!("Account not found {}", address);

            Self {
                address,
                account: pubkey,
                writable,
                new: true,
                size: 0,
                size_current: 0,
                additional_resize_steps: 0,
                data: None,
            }
        }
    }

    pub fn rpc_load(rpc_client: &dyn Rpc, evm_loader: &Pubkey, address: Address, writable: bool) -> Self {
        let (key, _) = make_solana_program_address(&address,  evm_loader);
        info!("get_account_from_solana {} => {}", address, key);

        let account = rpc_client.get_account(&key).ok();
        Self::new(address, key, account, writable)
    }
}

#[derive(serde::Serialize, Clone)]
pub struct SolanaAccount {
    #[serde(serialize_with = "serde_pubkey_bs58")]
    pubkey: Pubkey,
    is_writable: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BlockOverrides {
    pub number: Option<u64>,
    #[allow(unused)]
    pub difficulty: Option<U256>,  // NOT SUPPORTED by Neon EVM
    pub time: Option<i64>,
    #[allow(unused)]
    pub gas_limit: Option<u64>,    // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub coinbase: Option<Address>, // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub random: Option<U256>,      // NOT SUPPORTED BY Neon EVM
    #[allow(unused)]
    pub base_fee: Option<U256>,    // NOT SUPPORTED BY Neon EVM
}

#[derive(Debug, Clone, serde::Deserialize)]
pub enum StateOverride {
    NoOverride,
    State(HashMap<U256, [u8; 32]>),
    StateDiff(HashMap<U256, [u8; 32]>),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AccountOverride {
    pub nonce: Option<u64>,
    pub code: Option<Vec<u8>>,
    pub balance: Option<u64>,
    pub state_override: StateOverride,
}

impl AccountOverride {
    pub fn apply(&self, ether_account: &mut EthereumAccount) {
        if let Some(nonce) = self.nonce {
            ether_account.trx_count = nonce;
        }
        if let Some(balance) = self.balance {
            ether_account.balance = U256::from(balance);
        }
        if let Some(code) = &self.code {
            ether_account.code_size = code.len() as u32;
        }
    }
}

pub type AccountOverrides = HashMap<Address, AccountOverride>;

#[allow(clippy::module_name_repetitions)]
pub struct EmulatorAccountStorage<'a> {
    pub accounts: RefCell<HashMap<Address, NeonAccount>>,
    pub solana_accounts: RefCell<HashMap<Pubkey, SolanaAccount>>,
    rpc_client: &'a dyn Rpc,
    evm_loader: Pubkey,
    block_number: u64,
    block_timestamp: i64,
    neon_token_mint: Pubkey,
    chain_id: u64,
    state_override: Option<AccountOverrides>,
}

impl<'a> EmulatorAccountStorage<'a> {
    pub fn new(
        rpc_client: &'a dyn Rpc,
        evm_loader: Pubkey,
        token_mint: Pubkey,
        chain_id: u64,
        block_overrides: Option<BlockOverrides>,
        state_override: Option<AccountOverrides>,
    ) -> EmulatorAccountStorage {
        trace!("backend::new");

        let block_number = block_overrides.as_ref()
            .and_then(|overrides| overrides.number)
            .unwrap_or_else(|| rpc_client.get_slot().unwrap_or_default());

        let block_timestamp = block_overrides.as_ref()
            .and_then(|overrides| overrides.time)
            .unwrap_or_else(|| rpc_client.get_block_time(block_number).unwrap_or_default());

        Self {
            accounts: RefCell::new(HashMap::new()),
            solana_accounts: RefCell::new(HashMap::new()),
            rpc_client,
            evm_loader,
            block_number,
            block_timestamp,
            neon_token_mint: token_mint,
            chain_id,
            state_override,
        }
    }

    pub fn initialize_cached_accounts(&self, addresses: &[Address]) {
        let pubkeys: Vec<_> = addresses
            .iter()
            .map(|address| make_solana_program_address(address, &self.evm_loader).0)
            .collect();
        if let Ok(accounts) = self.rpc_client.get_multiple_accounts(&pubkeys) {
            let entries = addresses.iter().zip(accounts).zip(pubkeys);
            let mut accounts_storage = self.accounts.borrow_mut();
            for ((&address, account), pubkey) in entries {
                accounts_storage.insert(address, NeonAccount::new(address, pubkey, account, false));
            }
        }
    }

    pub fn get_account_from_solana(
        rpc_client: &dyn Rpc,
        evm_loader: &Pubkey,
        address: &Address,
    ) -> (Pubkey, Option<Account>) {
        let (solana_address, _solana_nonce) =
            make_solana_program_address(address, evm_loader);
        info!("get_account_from_solana {} => {}", address, solana_address);

        if let Ok(acc) = rpc_client.get_account(&solana_address) {
            trace!("Account found");
            trace!("Account data len {}", acc.data.len());
            trace!("Account owner {}", acc.owner);

            (solana_address, Some(acc))
        } else {
            warn!("Account not found {}", address);

            (solana_address, None)
        }
    }

    fn add_ethereum_account(&self, address: &Address, writable: bool) -> bool {
        if is_precompile_address(address) {
            return true;
        }

        let mut accounts = self.accounts.borrow_mut();

        if let Some(ref mut account) = accounts.get_mut(address) {
            account.writable |= writable;

            true
        } else {
            let account = NeonAccount::rpc_load(self.rpc_client, &self.evm_loader, *address, writable);
            accounts.insert(*address, account);

            false
        }
    }

    fn add_solana_account(&self, pubkey: Pubkey, is_writable: bool) {
        if solana_sdk::system_program::check_id(&pubkey) {
            return;
        }

        if pubkey == FAKE_OPERATOR {
            return;
        }

        let mut solana_accounts = self.solana_accounts.borrow_mut();

        let account = SolanaAccount {
            pubkey,
            is_writable,
        };
        if is_writable {
            solana_accounts.insert(pubkey, account);
        } else {
            solana_accounts.entry(pubkey).or_insert(account);
        }
    }

    #[must_use]
    pub fn apply_actions(&self, actions: &[Action]) -> u64 {
        info!("apply_actions");

        let mut gas = 0_u64;
        let rent = Rent::get().expect("Rent get error");

        for action in actions {
            #[allow(clippy::match_same_arms)]
            match action {
                Action::NeonTransfer {
                    source,
                    target,
                    value,
                } => {
                    info!("neon transfer {value} from {source} to {target}");

                    self.add_ethereum_account(source, true);
                    self.add_ethereum_account(target, true);
                }
                Action::NeonWithdraw { source, value } => {
                    info!("neon withdraw {value} from {source}");

                    self.add_ethereum_account(source, true);
                }
                Action::EvmSetStorage {
                    address,
                    index,
                    value,
                } => {
                    info!("set storage {address} -> {index} = {}", hex::encode(value));

                    if *index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT) {
                        self.add_ethereum_account(address, true);
                    } else {
                        let (base, _) = address.find_solana_address(self.program_id());
                        let storage_account =
                            EthereumStorageAddress::new(self.program_id(), &base, index);
                        self.add_solana_account(*storage_account.pubkey(), true);

                        if self.storage(address, index) == [0_u8; 32] {
                            let metadata_size = EthereumStorage::SIZE;
                            let element_size = 1 + std::mem::size_of_val(value);

                            let cost = rent.minimum_balance(metadata_size + element_size);
                            gas = gas.saturating_add(cost);
                        }
                    }
                }
                Action::EvmIncrementNonce { address } => {
                    info!("nonce increment {address}");

                    self.add_ethereum_account(address, true);
                }
                Action::EvmSetCode { address, code } => {
                    info!("set code {address} -> {} bytes", code.len());

                    self.add_ethereum_account(address, true);
                }
                Action::EvmSelfDestruct { address } => {
                    info!("selfdestruct {address}");

                    self.add_ethereum_account(address, true);
                }
                Action::ExternalInstruction {
                    program_id,
                    accounts,
                    allocate,
                    ..
                } => {
                    info!("external call {program_id}");

                    self.add_solana_account(*program_id, false);

                    for account in accounts {
                        self.add_solana_account(account.pubkey, account.is_writable);
                    }

                    if *allocate != 0 {
                        let cost = rent.minimum_balance(*allocate);
                        gas = gas.saturating_add(cost);
                    }
                }
            }
        }

        gas
    }

    #[must_use]
    pub fn apply_accounts_operations(&self, operations: AccountsOperations) -> u64 {
        let mut gas = 0_u64;
        let rent = Rent::get().expect("Rent get error");

        let mut iterations = 0_usize;

        let mut accounts = self.accounts.borrow_mut();
        for (address, operation) in operations {
            let new_size = match operation {
                AccountOperation::Create { space } => space,
                AccountOperation::Resize { to, .. } => to,
            };
            accounts.entry(address).and_modify(|a| {
                a.size = new_size;
                a.additional_resize_steps =
                    new_size.saturating_sub(a.size_current).saturating_sub(1)
                        / MAX_PERMITTED_DATA_INCREASE;
                iterations = iterations.max(a.additional_resize_steps);
            });

            let allocate_cost = rent.minimum_balance(new_size);
            gas = gas.saturating_add(allocate_cost);
        }

        let iterations_cost = (iterations as u64) * LAMPORTS_PER_SIGNATURE;

        gas.saturating_add(iterations_cost)
    }

    fn ethereum_account_map_or<F, R>(&self, address: &Address, default: R, f: F) -> R
    where
        F: FnOnce(&EthereumAccount) -> R,
    {
        self.add_ethereum_account(address, false);

        let mut accounts = self.accounts.borrow_mut();
        let solana_account = accounts.get_mut(address).expect("get account error");

        if let Some(account_data) = &mut solana_account.data {
            let info = account_info(&solana_account.account, account_data);
            EthereumAccount::from_account(&self.evm_loader, &info)
                .map(|mut ether_account| {
                    if let Some(account_overrides) = &self.state_override {
                        if let Some(account_override) = account_overrides.get(address) {
                            account_override.apply(&mut ether_account);
                        }
                    }
                    ether_account
                })
                .map_or(default, |a| f(&a))
        } else {
            default
        }
    }

    fn ethereum_contract_map_or<F, R>(&self, address: &Address, default: R, f: F) -> R
    where
        F: FnOnce(ether_contract::ContractData) -> R,
    {
        self.add_ethereum_account(address, false);

        let mut accounts = self.accounts.borrow_mut();
        let solana_account = accounts.get_mut(address).expect("get account error");

        if let Some(account_data) = &mut solana_account.data {
            let info = account_info(&solana_account.account, account_data);
            let account = EthereumAccount::from_account(&self.evm_loader, &info);
            match &account {
                Ok(a) => a.contract_data().map_or(default, f),
                Err(_) => default,
            }
        } else {
            default
        }
    }
}

impl<'a> AccountStorage for EmulatorAccountStorage<'a> {
    fn neon_token_mint(&self) -> &Pubkey {
        info!("neon_token_mint");
        &self.neon_token_mint
    }

    fn operator(&self) -> &Pubkey {
        info!("operator");
        &FAKE_OPERATOR
    }

    fn program_id(&self) -> &Pubkey {
        debug!("program_id");
        &self.evm_loader
    }

    fn block_number(&self) -> U256 {
        info!("block_number");
        self.block_number.into()
    }

    fn block_timestamp(&self) -> U256 {
        info!("block_timestamp");
        self.block_timestamp.try_into().unwrap()
    }

    fn block_hash(&self, number: U256) -> [u8; 32] {
        info!("block_hash {number}");

        let number = number.as_u64();

        self.add_solana_account(slot_hashes::ID, false);
        self.add_solana_account(recent_blockhashes::ID, false);

        if self.block_number <= number {
            return <[u8; 32]>::default();
        }

        if let Ok(slot_hashes_account) = self.rpc_client.get_account(&slot_hashes::ID) {
            if let Ok(recent_blockhashes_account) =
                self.rpc_client.get_account(&recent_blockhashes::ID)
            {
                let slot_hashes_data = slot_hashes_account.data;
                let slot_hashes_len = u64::from_le_bytes(slot_hashes_data[..8].try_into().unwrap());
                for i in 0..slot_hashes_len {
                    let offset = usize::try_from((i * 40) + 8).unwrap();
                    let slot =
                        u64::from_le_bytes(slot_hashes_data[offset..][..8].try_into().unwrap());
                    if number == slot {
                        let recent_blockhashes_data = recent_blockhashes_account.data;
                        if offset + 32 > recent_blockhashes_data.len() {
                            break;
                        }
                        return recent_blockhashes_data[offset..][..32].try_into().unwrap();
                    }
                }
            }
        }

        if let Ok(timestamp) = self.rpc_client.get_block(number) {
            let hash = bs58::decode(timestamp.blockhash).into_vec().unwrap();
            hash.try_into().unwrap()
        } else {
            warn!("Got error trying to get block hash");
            generate_fake_block_hash(number)
        }
    }

    fn exists(&self, address: &Address) -> bool {
        info!("exists {address}");

        self.add_ethereum_account(address, false);

        let accounts = self.accounts.borrow();
        accounts.contains_key(address)
    }

    fn nonce(&self, address: &Address) -> u64 {
        info!("nonce {address}");

        self.ethereum_account_map_or(address, 0_u64, |a| a.trx_count)
    }

    fn balance(&self, address: &Address) -> U256 {
        info!("balance {address}");

        self.ethereum_account_map_or(address, U256::ZERO, |a| a.balance)
    }

    fn code_size(&self, address: &Address) -> usize {
        info!("code_size {address}");

        self.ethereum_account_map_or(address, 0, |a| a.code_size as usize)
    }

    fn code_hash(&self, address: &Address) -> [u8; 32] {
        info!("code_hash {address}");

        solana_sdk::keccak::hash(&self.code(address)).to_bytes()
    }

    fn code(&self, address: &Address) -> evm_loader::evm::Buffer {
        use evm_loader::evm::Buffer;

        info!("code {address}");

        self.ethereum_contract_map_or(address, Buffer::empty(), |c| {
            if let Some(account_overrides) = &self.state_override {
                if let Some(account_override) = account_overrides.get(address) {
                    if let Some(code) = &account_override.code {
                        return Buffer::new(code);
                    }
                }
            }
            Buffer::new(&c.code())
        })
    }

    fn generation(&self, address: &Address) -> u32 {
        let value = self.ethereum_account_map_or(address, 0_u32, |c| c.generation);

        info!("account generation {address} - {value}");
        value
    }

    fn storage(&self, address: &Address, index: &U256) -> [u8; 32] {
        if let Some(account_overrides) = &self.state_override {
            if let Some(account_override) = account_overrides.get(address) {
                match &account_override.state_override {
                    StateOverride::NoOverride => (),
                    StateOverride::State(state) =>
                        return state.get(index)
                            .cloned()
                            .unwrap_or_default(),
                    StateOverride::StateDiff(state_diff) =>
                        if let Some(value) = state_diff.get(index) {
                            return *value;
                        }
                }
            }
        }
        let value = if *index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT) {
            let index: usize = index.as_usize() * 32;
            self.ethereum_contract_map_or(address, <[u8; 32]>::default(), |c| {
                c.storage()[index..index + 32].try_into().unwrap()
            })
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            let (base, _) = address.find_solana_address(self.program_id());
            let storage_address = EthereumStorageAddress::new(self.program_id(), &base, &index);

            self.add_solana_account(*storage_address.pubkey(), false);

            let rpc_response = self
                .rpc_client
                .get_account_with_commitment(
                    storage_address.pubkey(),
                    self.rpc_client.commitment(),
                )
                .expect("Error querying account from Solana");

            if let Some(mut account) = rpc_response.value {
                if solana_sdk::system_program::check_id(&account.owner) {
                    debug!("read storage system owned");
                    <[u8; 32]>::default()
                } else {
                    let account_info = account_info(storage_address.pubkey(), &mut account);
                    let storage =
                        EthereumStorage::from_account(&self.evm_loader, &account_info)
                            .expect("EthereumAccount ctor error");
                    if (storage.address != *address)
                        || (storage.index != index)
                        || (storage.generation != self.generation(address))
                    {
                        debug!("storage collision");
                        <[u8; 32]>::default()
                    } else {
                        storage.get(subindex)
                    }
                }
            } else {
                debug!("storage account doesn't exist");
                <[u8; 32]>::default()
            }
        };

        info!("storage {address} -> {index} = {}", hex::encode(value));

        value
    }

    fn solana_account_space(&self, address: &Address) -> Option<usize> {
        self.ethereum_account_map_or(address, None, |account| Some(account.info.data_len()))
    }

    fn chain_id(&self) -> u64 {
        info!("chain_id");

        self.chain_id
    }

    fn clone_solana_account(&self, address: &Pubkey) -> OwnedAccountInfo {
        info!("clone_solana_account {}", address);

        if address == &FAKE_OPERATOR {
            OwnedAccountInfo {
                key: FAKE_OPERATOR,
                is_signer: true,
                is_writable: false,
                lamports: 100 * 1_000_000_000,
                data: vec![],
                owner: solana_sdk::system_program::ID,
                executable: false,
                rent_epoch: 0,
            }
        } else {
            self.add_solana_account(*address, false);

            let mut account = self
                .rpc_client
                .get_account(address)
                .unwrap_or_default();
            let info = account_info(address, &mut account);

            OwnedAccountInfo::from_account_info(self.program_id(), &info)
        }
    }

    fn clone_solana_account_partial(
        &self,
        address: &Pubkey,
        offset: usize,
        len: usize,
    ) -> Option<OwnedAccountInfoPartial> {
        info!("clone_solana_account_partial {}", address);

        let account = self.clone_solana_account(address);

        Some(OwnedAccountInfoPartial {
            key: account.key,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
            lamports: account.lamports,
            data: account.data.get(offset..offset + len).map(<[u8]>::to_vec)?,
            data_offset: offset,
            data_total_len: account.data.len(),
            owner: account.owner,
            executable: account.executable,
            rent_epoch: account.rent_epoch,
        })
    }
}

/// Creates new instance of `AccountInfo` from `Account`.
pub fn account_info<'a>(key: &'a Pubkey, account: &'a mut Account) -> AccountInfo<'a> {
    AccountInfo {
        key,
        is_signer: false,
        is_writable: false,
        lamports: Rc::new(RefCell::new(&mut account.lamports)),
        data: Rc::new(RefCell::new(&mut account.data)),
        owner: &account.owner,
        executable: account.executable,
        rent_epoch: account.rent_epoch,
    }
}

pub fn make_solana_program_address(ether_address: &Address, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[&[ACCOUNT_SEED_VERSION], ether_address.as_bytes()],
        program_id,
    )
}
