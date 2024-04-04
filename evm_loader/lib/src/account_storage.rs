use async_trait::async_trait;
use elsa::FrozenMap;
use solana_client::rpc_response::{Response, RpcResponseContext, RpcResult};
use std::collections::HashSet;
use std::{
    cell::{RefCell, RefMut},
    convert::TryInto,
    rc::Rc,
};

use crate::{
    account_data::AccountData, rpc::Rpc, solana_simulator::SolanaSimulator, NeonError, NeonResult,
};
use ethnum::U256;
pub use evm_loader::account_storage::{AccountStorage, SyncedAccountStorage};
use evm_loader::{
    account::{
        legacy::{LegacyEtherData, LegacyStorageData},
        BalanceAccount, ContractAccount, StorageCell, StorageCellAddress,
    },
    account_storage::find_slot_hash,
    config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
    error::Error as EvmLoaderError,
    executor::OwnedAccountInfo,
    types::Address,
};
use log::{debug, info, trace};
use serde::{Deserialize, Serialize};
use solana_client::client_error::{self, Result as ClientResult};
use solana_sdk::{
    account::Account,
    account_info::{AccountInfo, IntoAccountInfo},
    clock::{Clock, Slot, UnixTimestamp},
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    program_error::ProgramError,
    pubkey,
    pubkey::{Pubkey, PubkeyError},
    rent::Rent,
    system_program,
    sysvar::slot_hashes,
    transaction_context::TransactionReturnData,
};

use crate::commands::get_config::{BuildConfigSimulator, ChainInfo};
use crate::tracing::{AccountOverrides, BlockOverrides};
use serde_with::{serde_as, DisplayFromStr};

const FAKE_OPERATOR: Pubkey = pubkey!("neonoperator1111111111111111111111111111111");

#[derive(Default, Clone, Copy)]
pub struct ExecuteStatus {
    pub external_solana_calls: bool,
    pub reverts_before_solana_calls: bool,
    pub reverts_after_solana_calls: bool,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaAccount {
    #[serde_as(as = "DisplayFromStr")]
    pub pubkey: Pubkey,
    pub is_writable: bool,
    pub is_legacy: bool,
}

#[allow(clippy::module_name_repetitions)]
pub struct EmulatorAccountStorage<'rpc, T: Rpc> {
    accounts: FrozenMap<Pubkey, Box<RefCell<AccountData>>>,
    call_stack: Vec<FrozenMap<Pubkey, Box<RefCell<AccountData>>>>,

    pub gas: u64,
    pub realloc_iterations: u64,
    pub execute_status: ExecuteStatus,
    rpc: &'rpc T,
    program_id: Pubkey,
    operator: Pubkey,
    chains: Vec<ChainInfo>,
    block_number: u64,
    block_timestamp: i64,
    timestamp_used: RefCell<bool>,
    rent: Rent,
    _state_overrides: Option<AccountOverrides>,
    accounts_cache: FrozenMap<Pubkey, Box<Option<Account>>>,
    used_accounts: FrozenMap<Pubkey, Box<RefCell<SolanaAccount>>>,
    return_data: RefCell<Option<TransactionReturnData>>,
}

#[async_trait(?Send)]
impl<'rpc, T: Rpc> Rpc for EmulatorAccountStorage<'rpc, T> {
    async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>> {
        let account = if *key == self.operator {
            Some(Account {
                lamports: 100 * 1_000_000_000,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            })
        } else if let Some(account) = self.accounts.get(key) {
            let acc = &*account.borrow();
            Some(acc.into())
        } else {
            self._get_account_from_rpc(*key).await?.cloned()
        };

        Ok(Response {
            context: RpcResponseContext {
                slot: self.block_number,
                api_version: None,
            },
            value: account,
        })
    }

    async fn get_account_with_commitment(
        &self,
        _key: &Pubkey,
        _commitment: CommitmentConfig,
    ) -> RpcResult<Option<Account>> {
        unimplemented!();
    }

    async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> ClientResult<Vec<Option<Account>>> {
        // TODO: Optimize this!!!
        let mut result = vec![];
        for key in pubkeys {
            let account = self.get_account(key).await?.value;
            result.push(account);
        }
        Ok(result)
    }

    async fn get_block_time(&self, _slot: Slot) -> ClientResult<UnixTimestamp> {
        // Ok(self.block_timestamp)
        unimplemented!();
    }

    async fn get_slot(&self) -> ClientResult<Slot> {
        //Ok(self.block_number)
        unimplemented!();
    }
}

impl<'rpc, T: Rpc + BuildConfigSimulator> EmulatorAccountStorage<'rpc, T> {
    pub async fn new(
        rpc: &'rpc T,
        program_id: Pubkey,
        chains: Option<Vec<ChainInfo>>,
        block_overrides: Option<BlockOverrides>,
        state_overrides: Option<AccountOverrides>,
    ) -> Result<EmulatorAccountStorage<T>, NeonError> {
        trace!("backend::new");

        let block_number = match block_overrides.as_ref().and_then(|o| o.number) {
            None => rpc.get_slot().await?,
            Some(number) => number,
        };

        let block_timestamp = match block_overrides.as_ref().and_then(|o| o.time) {
            None => rpc.get_block_time(block_number).await?,
            Some(time) => time,
        };

        let chains = match chains {
            None => crate::commands::get_config::read_chains(rpc, program_id).await?,
            Some(chains) => chains,
        };

        let rent_account = rpc
            .get_account(&solana_sdk::sysvar::rent::id())
            .await?
            .value
            .ok_or(NeonError::AccountNotFound(solana_sdk::sysvar::rent::id()))?;
        let rent = bincode::deserialize::<Rent>(&rent_account.data)?;
        info!("Rent: {rent:?}");

        Ok(Self {
            accounts: FrozenMap::new(),
            call_stack: vec![],
            program_id,
            operator: FAKE_OPERATOR,
            chains,
            gas: 0,
            realloc_iterations: 0,
            execute_status: ExecuteStatus::default(),
            rpc,
            block_number,
            block_timestamp,
            timestamp_used: RefCell::new(false),
            _state_overrides: state_overrides,
            rent,
            accounts_cache: FrozenMap::new(),
            used_accounts: FrozenMap::new(),
            return_data: RefCell::new(None),
        })
    }

    pub fn new_from_other(other: &Self, block_shift: u64, timestamp_shift: i64) -> Self {
        Self {
            accounts: FrozenMap::new(),
            call_stack: vec![],
            program_id: other.program_id,
            operator: other.operator,
            chains: other.chains.clone(),
            gas: 0,
            realloc_iterations: 0,
            execute_status: ExecuteStatus::default(),
            rpc: other.rpc,
            block_number: other.block_number.saturating_add(block_shift),
            block_timestamp: other.block_timestamp.saturating_add(timestamp_shift),
            timestamp_used: RefCell::new(false),
            rent: other.rent,
            _state_overrides: other._state_overrides.clone(),
            accounts_cache: other.accounts_cache.clone(),
            used_accounts: other.used_accounts.clone(),
            return_data: RefCell::new(None),
        }
    }

    pub async fn with_accounts(
        rpc: &'rpc T,
        program_id: Pubkey,
        accounts: &[Pubkey],
        chains: Option<Vec<ChainInfo>>,
        block_overrides: Option<BlockOverrides>,
        state_overrides: Option<AccountOverrides>,
    ) -> Result<EmulatorAccountStorage<'rpc, T>, NeonError> {
        let storage = Self::new(rpc, program_id, chains, block_overrides, state_overrides).await?;

        storage.download_accounts(accounts).await?;

        Ok(storage)
    }
}

impl<'a, T: Rpc> EmulatorAccountStorage<'_, T> {
    async fn download_accounts(&self, pubkeys: &[Pubkey]) -> Result<(), NeonError> {
        let accounts = self.rpc.get_multiple_accounts(pubkeys).await?;

        for (key, account) in pubkeys.iter().zip(accounts) {
            self.accounts_cache.insert(*key, Box::new(account));
        }

        Ok(())
    }

    pub async fn _get_account_from_rpc(
        &self,
        pubkey: Pubkey,
    ) -> client_error::Result<Option<&Account>> {
        if pubkey == FAKE_OPERATOR {
            return Ok(None);
        }

        if let Some(account) = self.accounts_cache.get(&pubkey) {
            return Ok(account.as_ref());
        }

        let response = self.rpc.get_account(&pubkey).await?;
        let account = self.accounts_cache.insert(pubkey, Box::new(response.value));
        Ok(account.as_ref())
    }

    fn mark_account(&self, pubkey: Pubkey, is_writable: bool, is_legacy: bool) {
        let mut data = self
            .used_accounts
            .insert(
                pubkey,
                Box::new(RefCell::new(SolanaAccount {
                    pubkey,
                    is_writable: false,
                    is_legacy: false,
                })),
            )
            .borrow_mut();
        data.is_writable |= is_writable;
        data.is_legacy |= is_legacy;
    }

    async fn _add_legacy_account(
        &self,
        info: &AccountInfo<'_>,
    ) -> NeonResult<(&RefCell<AccountData>, &RefCell<AccountData>)> {
        let legacy = LegacyEtherData::from_account(&self.program_id, info)?;

        let (balance_pubkey, _) = legacy
            .address
            .find_balance_address(&self.program_id, self.default_chain_id());
        let balance_data = self.add_empty_account(balance_pubkey)?;
        if (legacy.balance > 0) || (legacy.trx_count > 0) {
            let mut balance_data = balance_data.borrow_mut();
            let mut balance = self.create_ethereum_balance(
                &mut balance_data,
                legacy.address,
                self.default_chain_id(),
            )?;
            balance.mint(legacy.balance)?;
            balance.increment_nonce_by(legacy.trx_count)?;
            self.mark_account(balance_pubkey, true, true);
        } else {
            self.mark_account(balance_pubkey, false, true);
        }

        let (contract_pubkey, _) = legacy.address.find_solana_address(&self.program_id);
        let contract_data = self.add_empty_account(contract_pubkey)?;
        if (legacy.code_size > 0) || (legacy.generation > 0) {
            let code = legacy.read_code(info);
            let storage = legacy.read_storage(info);

            let mut contract_data = contract_data.borrow_mut();
            let mut contract = self.create_ethereum_contract(
                &mut contract_data,
                legacy.address,
                self.default_chain_id(),
                legacy.generation,
                &code,
            )?;
            if !code.is_empty() {
                contract.set_storage_multiple_values(0, &storage);
            }
            self.mark_account(contract_pubkey, true, true);
        }

        // We have to mark account as writable, because we destroy the original legacy account
        self.mark_account(contract_pubkey, true, true);
        Ok((contract_data, balance_data))
    }

    async fn _get_contract_generation_limited(&self, address: Address) -> NeonResult<Option<u32>> {
        let extract_generation = |contract_data: &RefCell<AccountData>| -> NeonResult<Option<u32>> {
            let mut contract_data = contract_data.borrow_mut();
            if contract_data.is_empty() {
                Ok(None)
            } else {
                let contract = ContractAccount::from_account(
                    &self.program_id,
                    contract_data.into_account_info(),
                )?;
                if contract.code().len() > 0 {
                    Ok(Some(contract.generation()))
                } else {
                    Ok(None)
                }
            }
        };

        let (pubkey, _) = address.find_solana_address(&self.program_id);
        let contract_data = if let Some(contract_data) = self.accounts.get(&pubkey) {
            contract_data
        } else {
            let mut account = self._get_account_from_rpc(pubkey).await?.cloned();
            if let Some(account) = &mut account {
                let info = account_info(&pubkey, account);
                if *info.owner != self.program_id {
                    let account_data = AccountData::new_from_account(pubkey, account);
                    self.accounts
                        .insert(pubkey, Box::new(RefCell::new(account_data)))
                } else {
                    match evm_loader::account::tag(&self.program_id, &info)? {
                        evm_loader::account::TAG_ACCOUNT_CONTRACT => {
                            let data = AccountData::new_from_account(pubkey, account);
                            self.accounts.insert(pubkey, Box::new(RefCell::new(data)))
                        }
                        evm_loader::account::legacy::TAG_ACCOUNT_CONTRACT_DEPRECATED => self
                            ._add_legacy_account(&info)
                            .await
                            .map(|(contract, _balance)| contract)?,
                        _ => {
                            unimplemented!();
                        }
                    }
                }
            } else {
                self.add_empty_account(pubkey)?
            }
        };
        self.mark_account(pubkey, false, true);
        extract_generation(contract_data)
    }

    async fn _add_legacy_storage(
        &self,
        legacy_storage: &LegacyStorageData,
        info: &AccountInfo<'_>,
        pubkey: Pubkey,
    ) -> NeonResult<&RefCell<AccountData>> {
        let generation = self
            ._get_contract_generation_limited(legacy_storage.address)
            .await?;
        let storage_data = self.add_empty_account(pubkey)?;

        if Some(legacy_storage.generation) == generation {
            let cells = legacy_storage.read_cells(info);

            let mut storage_data = storage_data.borrow_mut();
            self.create_ethereum_storage(&mut storage_data)?;

            storage_data.expand(StorageCell::required_account_size(cells.len()));
            storage_data.lamports = self.rent.minimum_balance(storage_data.get_length());
            let mut storage =
                StorageCell::from_account(&self.program_id, storage_data.into_account_info())?;
            storage.cells_mut().copy_from_slice(&cells);
        }
        self.mark_account(pubkey, true, true);
        Ok(storage_data)
    }

    async fn add_account(
        &self,
        pubkey: Pubkey,
        account: &Account,
    ) -> NeonResult<&RefCell<AccountData>> {
        let mut account = account.clone();
        let info = account_info(&pubkey, &mut account);
        if *info.owner != self.program_id {
            let account_data = AccountData::new_from_account(pubkey, &account);
            self.mark_account(pubkey, false, false);
            Ok(self
                .accounts
                .insert(pubkey, Box::new(RefCell::new(account_data))))
        } else {
            let tag = evm_loader::account::tag(&self.program_id, &info)?;
            match tag {
                evm_loader::account::TAG_ACCOUNT_BALANCE
                | evm_loader::account::TAG_ACCOUNT_CONTRACT
                | evm_loader::account::TAG_STORAGE_CELL => {
                    // TODO: update header from previous revisions
                    let account_data = AccountData::new_from_account(pubkey, &account);
                    self.mark_account(pubkey, false, false);
                    Ok(self
                        .accounts
                        .insert(pubkey, Box::new(RefCell::new(account_data))))
                }
                evm_loader::account::legacy::TAG_ACCOUNT_CONTRACT_DEPRECATED => self
                    ._add_legacy_account(&info)
                    .await
                    .map(|(contract, _balance)| contract),
                evm_loader::account::legacy::TAG_STORAGE_CELL_DEPRECATED => {
                    let legacy_storage = LegacyStorageData::from_account(&self.program_id, &info)?;
                    self._add_legacy_storage(&legacy_storage, &info, pubkey)
                        .await
                }
                _ => {
                    unimplemented!();
                }
            }
        }
    }

    fn add_empty_account(&self, pubkey: Pubkey) -> NeonResult<&RefCell<AccountData>> {
        let account_data = AccountData::new(pubkey);
        self.mark_account(pubkey, false, false);
        Ok(self
            .accounts
            .insert(pubkey, Box::new(RefCell::new(account_data))))
    }

    async fn use_account(
        &self,
        pubkey: Pubkey,
        is_writable: bool,
    ) -> NeonResult<&RefCell<AccountData>> {
        if pubkey == self.operator() {
            return Err(EvmLoaderError::InvalidAccountForCall(pubkey).into());
        }

        self.mark_account(pubkey, is_writable, false);

        if let Some(account) = self.accounts.get(&pubkey) {
            return Ok(account);
        }

        let account = self._get_account_from_rpc(pubkey).await?;
        if let Some(account) = account {
            self.add_account(pubkey, account).await
        } else {
            self.add_empty_account(pubkey)
        }
    }

    async fn get_balance_account(
        &self,
        address: Address,
        chain_id: u64,
    ) -> NeonResult<&RefCell<AccountData>> {
        let (pubkey, _) = address.find_balance_address(self.program_id(), chain_id);

        if let Some(account) = self.accounts.get(&pubkey) {
            return Ok(account);
        }

        match self._get_account_from_rpc(pubkey).await? {
            Some(account) => self.add_account(pubkey, account).await,
            None => {
                if chain_id == self.default_chain_id() {
                    let (legacy_pubkey, _) = address.find_solana_address(self.program_id());
                    if self.accounts.get(&legacy_pubkey).is_some() {
                        // We already have information about contract account (empty or filled with data).
                        // So the balance should be updated, but it is missed. So return the empty account.
                        self.add_empty_account(pubkey)
                    } else {
                        // We didn't process contract account and we doesn't have any information about it.
                        // So we can try to process account which can be a legacy.
                        match self._get_account_from_rpc(legacy_pubkey).await? {
                            Some(legacy_account) => {
                                self.add_account(legacy_pubkey, legacy_account).await?;
                                match self.accounts.get(&pubkey) {
                                    Some(account) => Ok(account),
                                    None => self.add_empty_account(pubkey),
                                }
                            }
                            None => {
                                self.add_empty_account(legacy_pubkey)?;
                                self.add_empty_account(pubkey)
                            }
                        }
                    }
                } else {
                    self.add_empty_account(pubkey)
                }
            }
        }
    }

    async fn get_contract_account(&self, address: Address) -> NeonResult<&RefCell<AccountData>> {
        let (pubkey, _) = address.find_solana_address(self.program_id());

        if let Some(account) = self.accounts.get(&pubkey) {
            return Ok(account);
        }

        match self._get_account_from_rpc(pubkey).await? {
            Some(account) => self.add_account(pubkey, account).await,
            None => self.add_empty_account(pubkey),
        }
    }

    async fn get_storage_account(
        &self,
        address: Address,
        index: U256,
    ) -> NeonResult<&RefCell<AccountData>> {
        let (base, _) = address.find_solana_address(self.program_id());
        let cell_address = StorageCellAddress::new(self.program_id(), &base, &index);
        let cell_pubkey = *cell_address.pubkey();

        if let Some(account) = self.accounts.get(&cell_pubkey) {
            return Ok(account);
        }

        match self._get_account_from_rpc(cell_pubkey).await? {
            Some(account) => self.add_account(cell_pubkey, account).await,
            None => self.add_empty_account(cell_pubkey),
        }
    }

    pub async fn ethereum_balance_map_or<F, R>(
        &self,
        address: Address,
        chain_id: u64,
        default: R,
        action: F,
    ) -> NeonResult<R>
    where
        F: FnOnce(&BalanceAccount) -> R,
    {
        let mut balance_data = self
            .get_balance_account(address, chain_id)
            .await?
            .borrow_mut();
        if balance_data.is_empty() {
            Ok(default)
        } else {
            let account_info = balance_data.into_account_info();
            let balance = BalanceAccount::from_account(self.program_id(), account_info)?;
            Ok(action(&balance))
        }
    }

    pub async fn ethereum_contract_map_or<F, R>(
        &self,
        address: Address,
        default: R,
        action: F,
    ) -> NeonResult<R>
    where
        F: FnOnce(&ContractAccount) -> R,
    {
        let mut contract_data = self.get_contract_account(address).await?.borrow_mut();
        if contract_data.is_empty() {
            Ok(default)
        } else {
            let account_info = contract_data.into_account_info();
            let contract = ContractAccount::from_account(self.program_id(), account_info)?;
            Ok(action(&contract))
        }
    }

    pub async fn ethereum_storage_map_or<F, R>(
        &self,
        address: Address,
        index: U256,
        default: R,
        action: F,
    ) -> NeonResult<R>
    where
        F: FnOnce(&StorageCell) -> R,
    {
        let mut storage_data = self.get_storage_account(address, index).await?.borrow_mut();
        if storage_data.is_empty() {
            Ok(default)
        } else {
            let account_info = storage_data.into_account_info();
            let storage = StorageCell::from_account(self.program_id(), account_info)?;
            Ok(action(&storage))
        }
    }

    fn create_ethereum_balance(
        &'a self,
        account_data: &'a mut RefMut<AccountData>,
        address: Address,
        chain_id: u64,
    ) -> evm_loader::error::Result<BalanceAccount> {
        let required_len = BalanceAccount::required_account_size();
        account_data.assign(self.program_id)?;
        account_data.expand(required_len);
        account_data.lamports = self.rent.minimum_balance(account_data.get_length());

        BalanceAccount::initialize(
            account_data.into_account_info(),
            &self.program_id,
            address,
            chain_id,
        )
    }

    fn get_or_create_ethereum_balance(
        &'a self,
        account_data: &'a mut RefMut<AccountData>,
        address: Address,
        chain_id: u64,
    ) -> evm_loader::error::Result<BalanceAccount> {
        if account_data.is_empty() {
            self.create_ethereum_balance(account_data, address, chain_id)
        } else {
            BalanceAccount::from_account(&self.program_id, account_data.into_account_info())
        }
    }

    fn create_ethereum_contract(
        &'a self,
        account_data: &'a mut RefMut<AccountData>,
        address: Address,
        chain_id: u64,
        generation: u32,
        code: &[u8],
    ) -> evm_loader::error::Result<ContractAccount> {
        self.mark_account(account_data.pubkey, true, false);
        let required_len = ContractAccount::required_account_size(code);
        account_data.assign(self.program_id)?;
        account_data.expand(required_len);
        account_data.lamports = self.rent.minimum_balance(account_data.get_length());

        ContractAccount::initialize(
            account_data.into_account_info(),
            &self.program_id,
            address,
            chain_id,
            generation,
            code,
        )
    }

    fn create_ethereum_storage(
        &'a self,
        account_data: &'a mut RefMut<AccountData>,
    ) -> evm_loader::error::Result<StorageCell> {
        self.mark_account(account_data.pubkey, true, false);
        account_data.assign(self.program_id)?;
        account_data.expand(StorageCell::required_account_size(0));
        account_data.lamports = self.rent.minimum_balance(account_data.get_length());

        StorageCell::initialize(account_data.into_account_info(), &self.program_id)
    }

    fn get_or_create_ethereum_storage(
        &'a self,
        account_data: &'a mut RefMut<AccountData>,
    ) -> evm_loader::error::Result<StorageCell> {
        if account_data.is_empty() {
            self.create_ethereum_storage(account_data)
        } else {
            StorageCell::from_account(&self.program_id, account_data.into_account_info())
        }
    }

    async fn mint(
        &mut self,
        address: Address,
        chain_id: u64,
        value: U256,
    ) -> evm_loader::error::Result<()> {
        info!("mint {address}:{chain_id} {value}");
        let mut balance_data = self
            .get_balance_account(address, chain_id)
            .await
            .map_err(map_neon_error)?
            .borrow_mut();
        self.mark_account(balance_data.pubkey, true, false);

        let mut balance =
            self.get_or_create_ethereum_balance(&mut balance_data, address, chain_id)?;
        balance.mint(value)?;

        Ok(())
    }

    pub fn used_accounts(&self) -> Vec<SolanaAccount> {
        self.used_accounts
            .clone()
            .into_map()
            .values()
            .map(|v| v.borrow().clone())
            .collect::<Vec<_>>()
    }

    pub fn get_changes_in_rent(&self) -> evm_loader::error::Result<i64> {
        let accounts = self.accounts.clone();

        let mut changes_in_rent = 0i64;
        for (pubkey, account) in accounts.into_map().iter() {
            if *pubkey == system_program::ID {
                continue;
            }

            let (original_rent, original_size) =
                self.accounts_cache.get(pubkey).map_or((0, 0), |v| {
                    v.as_ref().map_or((0, 0), |v| (v.lamports, v.data.len()))
                });
            let new_acc = account.borrow();
            let new_rent = new_acc.lamports;
            let new_size = new_acc.get_length();
            info!("Changes in rent: {pubkey} {original_rent} -> {new_rent} | {original_size} -> {new_size}");

            if new_acc.is_busy() && new_rent < self.rent.minimum_balance(new_acc.get_length()) {
                info!("Account {pubkey} is not rent exempt");
                return Err(ProgramError::AccountNotRentExempt.into());
            }

            changes_in_rent += new_rent as i64 - original_rent as i64;
        }

        Ok(changes_in_rent)
    }

    pub fn is_timestamp_used(&self) -> bool {
        *self.timestamp_used.borrow()
    }
}

#[async_trait(?Send)]
impl<T: Rpc> AccountStorage for EmulatorAccountStorage<'_, T> {
    fn program_id(&self) -> &Pubkey {
        debug!("program_id");
        &self.program_id
    }

    fn operator(&self) -> Pubkey {
        info!("operator");
        self.operator
    }

    fn block_number(&self) -> U256 {
        info!("block_number");
        self.block_number.into()
    }

    fn block_timestamp(&self) -> U256 {
        info!("block_timestamp");
        *self.timestamp_used.borrow_mut() = true;
        self.block_timestamp.try_into().unwrap()
    }

    fn rent(&self) -> &Rent {
        &self.rent
    }

    fn return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        info!("return_data");
        self.return_data
            .borrow()
            .as_ref()
            .map(|data| (data.program_id, data.data.clone()))
    }

    fn set_return_data(&self, data: &[u8]) {
        info!("set_return_data");
        *self.return_data.borrow_mut() = Some(TransactionReturnData {
            program_id: self.program_id,
            data: data.to_vec(),
        });
    }

    async fn block_hash(&self, slot: u64) -> [u8; 32] {
        info!("block_hash {slot}");

        if let Ok(account) = self.use_account(slot_hashes::ID, false).await {
            let account_data = account.borrow();
            let data = account_data.data();
            if !data.is_empty() {
                return find_slot_hash(slot, data);
            }
        }
        panic!("Error querying account {} from Solana", slot_hashes::ID)
    }

    async fn nonce(&self, address: Address, chain_id: u64) -> u64 {
        info!("nonce {address}  {chain_id}");

        // TODO: move to reading data from Solana node
        // let nonce_override = self.account_override(address, |a| a.nonce);
        // if let Some(nonce_override) = nonce_override {
        //     return nonce_override;
        // }

        self.ethereum_balance_map_or(address, chain_id, u64::default(), |account| account.nonce())
            .await
            .unwrap()
    }

    async fn balance(&self, address: Address, chain_id: u64) -> U256 {
        info!("balance {address} {chain_id}");

        // TODO: move to reading data from Solana node
        // let balance_override = self.account_override(address, |a| a.balance);
        // if let Some(balance_override) = balance_override {
        //     return balance_override;
        // }

        self.ethereum_balance_map_or(address, chain_id, U256::default(), |account| {
            account.balance()
        })
        .await
        .unwrap()
    }

    fn is_valid_chain_id(&self, chain_id: u64) -> bool {
        for chain in &self.chains {
            if chain.id == chain_id {
                return true;
            }
        }

        false
    }

    fn chain_id_to_token(&self, chain_id: u64) -> Pubkey {
        for chain in &self.chains {
            if chain.id == chain_id {
                return chain.token;
            }
        }

        unreachable!();
    }

    fn default_chain_id(&self) -> u64 {
        for chain in &self.chains {
            if chain.name == "neon" {
                return chain.id;
            }
        }

        unreachable!();
    }

    async fn contract_chain_id(&self, address: Address) -> evm_loader::error::Result<u64> {
        let default_value = Err(EvmLoaderError::Custom(std::format!(
            "Account {address} - invalid tag"
        )));

        self.ethereum_contract_map_or(address, default_value, |a| Ok(a.chain_id()))
            .await
            .unwrap()
    }

    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8) {
        address.find_solana_address(self.program_id())
    }

    async fn code_size(&self, address: Address) -> usize {
        info!("code_size {address}");

        self.code(address).await.len()
    }

    async fn code(&self, address: Address) -> evm_loader::evm::Buffer {
        use evm_loader::evm::Buffer;

        info!("code {address}");

        // TODO: move to reading data from Solana node
        // let code_override = self.account_override(address, |a| a.code.clone());
        // if let Some(code_override) = code_override {
        //     return Buffer::from_vec(code_override.0);
        // }

        let code = self
            .ethereum_contract_map_or(address, Vec::default(), |c| c.code().to_vec())
            .await
            .unwrap();

        Buffer::from_vec(code)
    }

    async fn storage(&self, address: Address, index: U256) -> [u8; 32] {
        // TODO: move to reading data from Solana node
        // let storage_override = self.account_override(address, |a| a.storage(index));
        // if let Some(storage_override) = storage_override {
        //     return storage_override;
        // }

        let value = if index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
            let index: usize = index.as_usize();
            self.ethereum_contract_map_or(address, [0_u8; 32], |c| c.storage_value(index))
                .await
                .unwrap()
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            self.ethereum_storage_map_or(address, index, <[u8; 32]>::default(), |cell| {
                cell.get(subindex)
            })
            .await
            .unwrap()
        };

        info!("storage {address} -> {index} = {}", hex::encode(value));

        value
    }

    async fn clone_solana_account(&self, address: &Pubkey) -> OwnedAccountInfo {
        info!("clone_solana_account {}", address);

        if *address == self.operator() {
            OwnedAccountInfo {
                key: self.operator(),
                is_signer: true,
                is_writable: false,
                lamports: 100 * 1_000_000_000,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            }
        } else {
            let account = self
                .use_account(*address, false)
                .await
                .expect("Error querying account from Solana");

            let mut account_data = account.borrow_mut();
            let info = account_data.into_account_info();
            OwnedAccountInfo::from_account_info(self.program_id(), &info)
        }
    }

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&AccountInfo) -> R,
    {
        let account = self
            .use_account(*address, false)
            .await
            .expect("Error querying account from Solana");

        let mut account_data = account.borrow_mut();
        let info = account_data.into_account_info();
        action(&info)
    }
}

fn map_neon_error(e: NeonError) -> EvmLoaderError {
    EvmLoaderError::Custom(e.to_string())
}

#[async_trait(?Send)]
impl<T: Rpc> SyncedAccountStorage for EmulatorAccountStorage<'_, T> {
    async fn set_code(
        &mut self,
        address: Address,
        chain_id: u64,
        code: Vec<u8>,
    ) -> evm_loader::error::Result<()> {
        info!("set_code {address} -> {} bytes", code.len());
        {
            let mut account_data = self
                .get_contract_account(address)
                .await
                .map_err(map_neon_error)?
                .borrow_mut();
            let pubkey = account_data.pubkey;

            if account_data.is_empty() {
                self.create_ethereum_contract(&mut account_data, address, chain_id, 0, &code)?;
            } else {
                let contract = ContractAccount::from_account(
                    self.program_id(),
                    account_data.into_account_info(),
                )?;
                if contract.code().len() > 0 {
                    return Err(EvmLoaderError::AccountAlreadyInitialized(
                        account_data.pubkey,
                    ));
                }
                let new_account_data = RefCell::new(AccountData::new(pubkey));
                {
                    let mut new_account = new_account_data.borrow_mut();
                    let mut new_contract = self.create_ethereum_contract(
                        &mut new_account,
                        address,
                        chain_id,
                        contract.generation(),
                        &code,
                    )?;
                    let storage = *contract.storage();
                    new_contract.set_storage_multiple_values(0, &storage);
                }
                *account_data = new_account_data.replace_with(|_| AccountData::new(pubkey));
            }
        }

        let realloc = ContractAccount::required_account_size(&code)
            / solana_sdk::entrypoint::MAX_PERMITTED_DATA_INCREASE;
        self.realloc_iterations = self.realloc_iterations.max(realloc as u64);

        Ok(())
    }

    async fn set_storage(
        &mut self,
        address: Address,
        index: U256,
        value: [u8; 32],
    ) -> evm_loader::error::Result<()> {
        info!("set_storage {address} -> {index} = {}", hex::encode(value));
        const STATIC_STORAGE_LIMIT: U256 = U256::new(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u128);

        if index < STATIC_STORAGE_LIMIT {
            let mut contract_data = self
                .get_contract_account(address)
                .await
                .map_err(map_neon_error)?
                .borrow_mut();

            let mut contract = if contract_data.is_empty() {
                self.create_ethereum_contract(&mut contract_data, address, 0, 0, &[])?
            } else {
                ContractAccount::from_account(self.program_id(), contract_data.into_account_info())?
            };
            contract.set_storage_value(index.as_usize(), &value);
            self.mark_account(contract_data.pubkey, true, false);
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            let mut storage_data = self
                .get_storage_account(address, index)
                .await
                .map_err(map_neon_error)?
                .borrow_mut();

            let mut storage = self.get_or_create_ethereum_storage(&mut storage_data)?;
            storage.update(subindex, &value)?;
            self.mark_account(storage_data.pubkey, true, false);
            storage_data.lamports = self.rent.minimum_balance(storage_data.get_length());
        }

        Ok(())
    }

    async fn increment_nonce(
        &mut self,
        address: Address,
        chain_id: u64,
    ) -> evm_loader::error::Result<()> {
        info!("nonce increment {address} {chain_id}");
        let mut balance_data = self
            .get_balance_account(address, chain_id)
            .await
            .map_err(map_neon_error)?
            .borrow_mut();
        let mut balance =
            self.get_or_create_ethereum_balance(&mut balance_data, address, chain_id)?;
        balance.increment_nonce()?;
        self.mark_account(balance_data.pubkey, true, false);

        Ok(())
    }

    async fn transfer(
        &mut self,
        from_address: Address,
        to_address: Address,
        chain_id: u64,
        value: U256,
    ) -> evm_loader::error::Result<()> {
        self.burn(from_address, chain_id, value).await?;
        self.mint(to_address, chain_id, value).await?;

        Ok(())
    }

    async fn burn(
        &mut self,
        address: Address,
        chain_id: u64,
        value: U256,
    ) -> evm_loader::error::Result<()> {
        info!("burn {address} {chain_id} {value}");
        let mut balance_data = self
            .get_balance_account(address, chain_id)
            .await
            .map_err(map_neon_error)?
            .borrow_mut();
        self.mark_account(balance_data.pubkey, true, false);

        let mut balance =
            self.get_or_create_ethereum_balance(&mut balance_data, address, chain_id)?;
        balance.burn(value)?;

        Ok(())
    }

    async fn execute_external_instruction(
        &mut self,
        instruction: Instruction,
        seeds: Vec<Vec<Vec<u8>>>,
        _fee: u64,
        emulated_internally: bool,
    ) -> evm_loader::error::Result<()> {
        use solana_sdk::{message::Message, signature::Signer, transaction::Transaction};

        info!("execute_external_instruction: {instruction:?}");
        info!("Operator: {}", self.operator);
        self.execute_status.external_solana_calls |= !emulated_internally;

        let mut solana_simulator = SolanaSimulator::new(self)
            .await
            .map_err(|e| EvmLoaderError::Custom(e.to_string()))?;

        solana_simulator.set_sysvar(&Clock {
            slot: self.block_number,
            epoch_start_timestamp: self.block_timestamp,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: self.block_timestamp,
        });

        let signers = seeds
            .iter()
            .map(|s| {
                let seed = s.iter().map(|s| s.as_slice()).collect::<Vec<_>>();
                let signer = Pubkey::create_program_address(&seed, &self.program_id)?;
                Ok(signer)
            })
            .collect::<Result<HashSet<_>, PubkeyError>>()?;
        info!("Signers: {signers:?}");

        let mut accounts = Vec::new();
        accounts.push(instruction.program_id);
        self.mark_account(instruction.program_id, false, false);

        for meta in instruction.accounts.iter() {
            if meta.pubkey != self.operator {
                self.use_account(meta.pubkey, meta.is_writable)
                    .await
                    .map_err(map_neon_error)?;
                if meta.is_signer && !signers.contains(&meta.pubkey) {
                    return Err(ProgramError::MissingRequiredSignature.into());
                }
            }
            accounts.push(meta.pubkey);
        }

        solana_simulator
            .sync_accounts(self, &accounts)
            .await
            .map_err(|e| EvmLoaderError::Custom(e.to_string()))?;

        let trx = Transaction::new_unsigned(Message::new_with_blockhash(
            &[instruction.clone()],
            Some(&solana_simulator.payer().pubkey()),
            &solana_simulator.blockhash(),
        ));

        let result = solana_simulator
            .simulate_legacy_transaction(trx)
            .map_err(|e| EvmLoaderError::Custom(e.to_string()))?;

        if let Err(error) = result.result {
            return Err(EvmLoaderError::ExternalCallFailed(
                instruction.program_id,
                error.to_string(),
            ));
        }

        if let Some(return_data) = result.return_data {
            *self.return_data.borrow_mut() = Some(return_data);
        }

        for meta in instruction.accounts.iter() {
            if meta.pubkey == self.operator {
                continue;
            }
            let account = result
                .post_simulation_accounts
                .iter()
                .find(|(pubkey, _)| *pubkey == meta.pubkey)
                .map(|(_, account)| account)
                .ok_or_else(|| {
                    EvmLoaderError::Custom(format!("Account {} not found", meta.pubkey))
                })?;

            let mut account_data = self
                .accounts
                .get(&meta.pubkey)
                .ok_or_else(|| {
                    EvmLoaderError::Custom(format!("Account data {} not found", meta.pubkey))
                })?
                .borrow_mut();

            *account_data = AccountData::new_from_account(meta.pubkey, account);
        }

        Ok(())
    }

    fn snapshot(&mut self) {
        info!("snapshot");
        self.call_stack.push(self.accounts.clone());
    }

    fn revert_snapshot(&mut self) {
        info!("revert_snapshot");
        self.accounts = self.call_stack.pop().expect("No snapshots to revert");

        if self.execute_status.external_solana_calls {
            self.execute_status.reverts_before_solana_calls = true;
        } else {
            self.execute_status.reverts_after_solana_calls = true;
        }
    }

    fn commit_snapshot(&mut self) {
        self.call_stack.pop().expect("No snapshots to commit");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracing::AccountOverride;
    use hex_literal::hex;
    use std::collections::HashMap;
    use std::str::FromStr;

    mod mock_rpc_client {
        use crate::commands::get_config::BuildConfigSimulator;
        use crate::NeonResult;
        use crate::{commands::get_config::ConfigSimulator, rpc::Rpc};
        use async_trait::async_trait;
        use solana_client::client_error::Result as ClientResult;
        use solana_client::rpc_response::{Response, RpcResponseContext, RpcResult};
        use solana_sdk::account::Account;
        use solana_sdk::clock::{Slot, UnixTimestamp};
        use solana_sdk::commitment_config::CommitmentConfig;
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
            async fn get_account(&self, key: &Pubkey) -> RpcResult<Option<Account>> {
                let result = self.accounts.get(key).cloned();
                Ok(Response {
                    context: RpcResponseContext {
                        slot: 0,
                        api_version: None,
                    },
                    value: result,
                })
            }

            async fn get_account_with_commitment(
                &self,
                key: &Pubkey,
                _commitment: CommitmentConfig,
            ) -> RpcResult<Option<Account>> {
                self.get_account(key).await
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
        }

        #[async_trait(?Send)]
        impl BuildConfigSimulator for MockRpcClient {
            async fn build_config_simulator(
                &self,
                _program_id: Pubkey,
            ) -> NeonResult<ConfigSimulator> {
                unimplemented!();
            }
        }
    }

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
        let data_length = if (code.len() > 0) || (generation > 0) {
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
        *code_size_ptr = (code.len() as u32).to_le_bytes();
        *rw_blocked_ptr = 0u8.to_le_bytes();

        if (generation > 0) || (code.len() > 0) {
            let storage_offset = 1 + LegacyEtherData::SIZE;
            const STORAGE_LENGTH: usize = 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;
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
            data: data,
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
        pub fn required_account_size(count: usize) -> usize {
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
                data: data,
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
                self.address.find_solana_address(&program_id).0,
                create_legacy_ether_account(
                    &program_id,
                    &rent,
                    self.address,
                    self.balance,
                    self.nonce,
                ),
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
                self.address.find_solana_address(&program_id).0,
                create_legacy_ether_contract(
                    &program_id,
                    &rent,
                    self.address,
                    self.balance,
                    self.nonce,
                    self.generation,
                    &self.code,
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
            let (pubkey, _) = self
                .address
                .find_balance_address(&program_id, self.chain_id);
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
            let (pubkey, _) = self.address.find_solana_address(&program_id);
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

    const MISSING_STORAGE_INDEX: U256 = U256::new(1 * 256u128);
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
    }

    impl Fixture {
        pub async fn new() -> Self {
            let rent = Rent::default();
            let program_id =
                Pubkey::from_str("53DfF883gyixYNXnM7s5xhdeyV8mVk9T4i2hGV9vG9io").unwrap();
            let accounts = vec![
                (
                    Pubkey::from_str("SysvarRent111111111111111111111111111111111").unwrap(),
                    Account {
                        lamports: 1009200,
                        data: bincode::serialize(&rent).unwrap(),
                        owner: Pubkey::from_str("Sysvar1111111111111111111111111111111111111")
                            .unwrap(),
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
            }
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
            let data_length = code_len
                .map(|len| {
                    1 + LegacyEtherData::SIZE + 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT + len
                })
                .unwrap_or(1 + LegacyEtherData::SIZE);
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

        pub fn verify_rent_changes(&self, added_rent: u64, removed_rent: u64) {
            let changes = added_rent as i64 - removed_rent as i64;
            assert_eq!(self.get_changes_in_rent().unwrap(), changes);
        }
    }

    #[tokio::test]
    async fn test_read_balance_missing_account() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_balance_missing_account_extra_chain() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_balance_actual_account() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_balance_actual_account_extra_chain() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_balance_legacy_account() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(fixture.balance_rent(), fixture.legacy_rent(None));
    }

    #[tokio::test]
    async fn test_modify_actual_and_missing_account() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let from = &ACTUAL_BALANCE;
        let amount = U256::new(10);
        assert_eq!(from.chain_id, LEGACY_CHAIN_ID);
        assert_eq!(
            storage
                .transfer(from.address, MISSING_ADDRESS, from.chain_id, amount)
                .await
                .is_ok(),
            true
        );

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
        storage.verify_rent_changes(fixture.balance_rent(), 0);

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
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let from = &ACTUAL_BALANCE2;
        let amount = U256::new(11);
        assert_eq!(from.chain_id, EXTRA_CHAIN_ID);
        assert_eq!(
            storage
                .transfer(from.address, MISSING_ADDRESS, from.chain_id, amount)
                .await
                .is_ok(),
            true
        );

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
        storage.verify_rent_changes(fixture.balance_rent(), 0);

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
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let from = &ACTUAL_BALANCE;
        let to = &LEGACY_ACCOUNT;
        let amount = U256::new(10);
        assert_eq!(from.chain_id, LEGACY_CHAIN_ID);
        assert_eq!(
            storage
                .transfer(from.address, to.address, from.chain_id, amount)
                .await
                .is_ok(),
            true
        );

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
        storage.verify_rent_changes(fixture.balance_rent(), fixture.legacy_rent(None));

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
        let fixture = Fixture::new().await;
        let storage = fixture.build_account_storage().await;

        assert_eq!(*storage.code(MISSING_ADDRESS).await, [0u8; 0]);
        assert_eq!(
            storage.storage(MISSING_ADDRESS, U256::ZERO).await,
            [0u8; 32]
        );
        storage.verify_used_accounts(&[(fixture.contract_pubkey(MISSING_ADDRESS), false, false)]);
        storage.verify_rent_changes(0, 0);

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
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(LEGACY_CONTRACT.code),
            fixture.legacy_rent(Some(LEGACY_CONTRACT.code.len())),
        );
    }

    #[tokio::test]
    async fn test_read_legacy_contract_no_balance() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.contract_rent(contract.code),
            fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_read_actual_suicide_contract() {
        let fixture = Fixture::new().await;
        let storage = fixture.build_account_storage().await;

        let contract = &ACTUAL_SUICIDE;
        assert_eq!(*storage.code(contract.address).await, [0u8; 0]);
        assert_eq!(
            storage.storage(contract.address, U256::ZERO).await,
            [0u8; 32]
        );
        storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), false, false)]);
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_legacy_suicide_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(contract.code),
            fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_deploy_at_missing_contract() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let code = hex!("14643165").to_vec();
        assert_eq!(
            storage
                .set_code(MISSING_ADDRESS, LEGACY_CHAIN_ID, code.clone())
                .await
                .is_ok(),
            true
        );
        storage.verify_used_accounts(&[(fixture.contract_pubkey(MISSING_ADDRESS), true, false)]);
        storage.verify_rent_changes(fixture.contract_rent(&code), 0);
    }

    #[tokio::test]
    async fn test_deploy_at_actual_balance() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let code = hex!("14643165").to_vec();
        let acc = &ACTUAL_BALANCE;
        assert_eq!(
            storage
                .set_code(acc.address, LEGACY_CHAIN_ID, code.clone())
                .await
                .is_ok(),
            true
        );
        storage.verify_used_accounts(&[(fixture.contract_pubkey(acc.address), true, false)]);
        storage.verify_rent_changes(fixture.contract_rent(&code), 0);
    }

    #[tokio::test]
    async fn test_deploy_at_actual_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_deploy_at_legacy_account() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let code = hex!("37455846").to_vec();
        let contract = &LEGACY_ACCOUNT;
        assert_eq!(
            storage
                .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
                .await
                .is_ok(),
            true
        );
        storage.verify_used_accounts(&[
            (
                fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
                true,
                true,
            ),
            (fixture.contract_pubkey(contract.address), true, true),
        ]);
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(&code),
            fixture.legacy_rent(None),
        );
    }

    #[tokio::test]
    async fn test_deploy_at_legacy_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(contract.code),
            fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_deploy_at_actual_suicide() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let code = hex!("13412971").to_vec();
        let contract = &ACTUAL_SUICIDE;
        // TODO: Should we deploy new contract by the previous address?
        assert_eq!(
            storage
                .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
                .await
                .is_ok(),
            true,
        );
        storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), true, false)]);
        storage.verify_rent_changes(
            fixture.contract_rent(&code),
            fixture.contract_rent(contract.code),
        );
    }

    #[tokio::test]
    async fn test_deploy_at_legacy_suicide() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let code = hex!("13412971").to_vec();
        let contract = &LEGACY_SUICIDE;
        // TODO: Should we deploy new contract by the previous address?
        assert_eq!(
            storage
                .set_code(contract.address, LEGACY_CHAIN_ID, code.clone())
                .await
                .is_ok(),
            true,
        );
        storage.verify_used_accounts(&[
            (
                fixture.balance_pubkey(contract.address, LEGACY_CHAIN_ID),
                true,
                true,
            ),
            (fixture.contract_pubkey(contract.address), true, true),
        ]);
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(&code),
            fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_read_missing_storage_for_missing_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_missing_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_actual_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_modify_new_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let contract = &ACTUAL_CONTRACT;
        assert_eq!(
            storage
                .storage(contract.address, ACTUAL_STORAGE_INDEX + 1)
                .await,
            [0u8; 32]
        );
        storage.verify_rent_changes(0, 0);

        let new_value = [0x01u8; 32];
        assert_eq!(
            storage
                .set_storage(
                    contract.address,
                    ACTUAL_STORAGE_INDEX + 1,
                    new_value.clone()
                )
                .await
                .is_ok(),
            true
        );
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
        storage.verify_rent_changes(fixture.storage_rent(2), fixture.storage_rent(1));
    }

    #[tokio::test]
    async fn test_modify_missing_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let contract = &ACTUAL_CONTRACT;
        let new_value = [0x02u8; 32];
        assert_eq!(
            storage
                .set_storage(contract.address, MISSING_STORAGE_INDEX, new_value.clone())
                .await
                .is_ok(),
            true
        );
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
        storage.verify_rent_changes(fixture.storage_rent(1), 0);
    }

    #[tokio::test]
    async fn test_modify_internal_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
        let mut storage = fixture.build_account_storage().await;

        let contract = &ACTUAL_CONTRACT;
        let new_value = [0x03u8; 32];
        let index = U256::new(0);
        assert_eq!(
            storage
                .set_storage(contract.address, index, new_value.clone())
                .await
                .is_ok(),
            true
        );
        assert_eq!(storage.storage(contract.address, index).await, new_value);
        storage.verify_used_accounts(&[(fixture.contract_pubkey(contract.address), true, false)]);
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_legacy_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(fixture.storage_rent(1), fixture.legacy_storage_rent(1))
    }

    #[tokio::test]
    async fn test_read_outdate_storage_for_actual_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, fixture.legacy_storage_rent(1));
    }

    #[tokio::test]
    async fn test_read_missing_storage_for_legacy_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_legacy_storage_for_legacy_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(contract.code) + fixture.storage_rent(1),
            fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_read_outdate_storage_for_legacy_contract() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(contract.code),
            fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
        );
    }

    #[tokio::test]
    async fn test_read_missing_storage_for_legacy_suicide() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(0, 0);
    }

    #[tokio::test]
    async fn test_read_outdate_storage_for_legacy_suicide() {
        let fixture = Fixture::new().await;
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
        storage.verify_rent_changes(
            fixture.balance_rent() + fixture.contract_rent(contract.code),
            fixture.legacy_storage_rent(1) + fixture.legacy_rent(Some(contract.code.len())),
        )
    }
}
