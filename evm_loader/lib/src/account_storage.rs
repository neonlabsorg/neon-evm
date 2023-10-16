use async_trait::async_trait;
use evm_loader::account_storage::find_slot_hash;
use evm_loader::types::Address;
use solana_sdk::rent::Rent;
use solana_sdk::sysvar::{slot_hashes, Sysvar};
use std::collections::HashSet;
use std::{cell::RefCell, collections::HashMap, convert::TryInto, rc::Rc};

use crate::{rpc::Rpc, NeonError};
use ethnum::U256;
use evm_loader::evm::tracing::{AccountOverride, AccountOverrides, BlockOverrides};
use evm_loader::{
    account::{BalanceAccount, ContractAccount, StorageCell, StorageCellAddress},
    account_storage::AccountStorage,
    config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT,
    executor::{Action, OwnedAccountInfo},
};
use log::{debug, info, trace};
use serde::{Deserialize, Serialize};
use solana_client::client_error;
use solana_sdk::{account::Account, account_info::AccountInfo, pubkey, pubkey::Pubkey};

use crate::commands::get_config::ChainInfo;
use serde_with::{serde_as, DisplayFromStr};

const FAKE_OPERATOR: Pubkey = pubkey!("neonoperator1111111111111111111111111111111");

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaAccount {
    #[serde_as(as = "DisplayFromStr")]
    pubkey: Pubkey,
    is_writable: bool,
    #[serde(skip)]
    data: Option<Account>,
}

#[allow(clippy::module_name_repetitions)]
pub struct EmulatorAccountStorage<'rpc> {
    pub accounts: RefCell<HashMap<Pubkey, SolanaAccount>>,
    pub gas: u64,
    rpc_client: &'rpc dyn Rpc,
    program_id: Pubkey,
    chains: Vec<ChainInfo>,
    block_number: u64,
    block_timestamp: i64,
    state_overrides: Option<AccountOverrides>,
}

impl<'rpc> EmulatorAccountStorage<'rpc> {
    pub async fn new(
        rpc_client: &'rpc dyn Rpc,
        program_id: Pubkey,
        block_overrides: Option<BlockOverrides>,
        state_overrides: Option<AccountOverrides>,
    ) -> Result<EmulatorAccountStorage<'rpc>, NeonError> {
        trace!("backend::new");

        let block_number = match block_overrides.as_ref().and_then(|o| o.number) {
            None => rpc_client.get_slot().await?,
            Some(number) => number,
        };

        let block_timestamp = match block_overrides.as_ref().and_then(|o| o.time) {
            None => rpc_client.get_block_time(block_number).await?,
            Some(time) => time,
        };

        let config = crate::commands::get_config::execute(rpc_client, program_id).await?;
        info!("{:?}", config);

        Ok(Self {
            accounts: RefCell::new(HashMap::new()),
            program_id,
            chains: config.chains,
            gas: 0,
            rpc_client,
            block_number,
            block_timestamp,
            state_overrides,
        })
    }

    pub async fn with_accounts(
        rpc_client: &'rpc dyn Rpc,
        program_id: Pubkey,
        accounts: &[Pubkey],
        block_overrides: Option<BlockOverrides>,
        state_overrides: Option<AccountOverrides>,
    ) -> Result<EmulatorAccountStorage<'rpc>, NeonError> {
        let storage = Self::new(rpc_client, program_id, block_overrides, state_overrides).await?;

        storage.download_accounts(accounts).await?;

        Ok(storage)
    }

    async fn download_accounts(&self, pubkeys: &[Pubkey]) -> Result<(), NeonError> {
        let accounts = self.rpc_client.get_multiple_accounts(pubkeys).await?;

        let mut cache = self.accounts.borrow_mut();

        for (key, account) in pubkeys.iter().zip(accounts) {
            let account = SolanaAccount {
                pubkey: *key,
                is_writable: false,
                data: account.clone(),
            };

            cache.insert(*key, account);
        }

        Ok(())
    }

    pub async fn use_account(
        &self,
        pubkey: Pubkey,
        is_writable: bool,
    ) -> client_error::Result<Option<Account>> {
        if let Some(account) = self.accounts.borrow_mut().get_mut(&pubkey) {
            account.is_writable |= is_writable;
            return Ok(account.data.clone());
        }

        let response = self.rpc_client.get_account(&pubkey).await?;
        let account = response.value;

        self.accounts.borrow_mut().insert(
            pubkey,
            SolanaAccount {
                pubkey,
                is_writable,
                data: account.clone(),
            },
        );

        Ok(account)
    }

    pub async fn use_balance_account(
        &self,
        address: Address,
        chain_id: u64,
        is_writable: bool,
    ) -> client_error::Result<(Pubkey, Option<Account>)> {
        let (pubkey, _) = address.find_balance_address(self.program_id(), chain_id);
        let account = self.use_account(pubkey, is_writable).await?;

        Ok((pubkey, account))
    }

    pub async fn use_contract_account(
        &self,
        address: Address,
        is_writable: bool,
    ) -> client_error::Result<(Pubkey, Option<Account>)> {
        let (pubkey, _) = address.find_solana_address(self.program_id());
        let account = self.use_account(pubkey, is_writable).await?;

        Ok((pubkey, account))
    }

    pub async fn use_storage_cell(
        &self,
        address: Address,
        index: U256,
        is_writable: bool,
    ) -> client_error::Result<(Pubkey, Option<Account>)> {
        let (base, _) = address.find_solana_address(self.program_id());
        let cell_address = StorageCellAddress::new(self.program_id(), &base, &index);

        let account = self
            .use_account(*cell_address.pubkey(), is_writable)
            .await?;

        Ok((*cell_address.pubkey(), account))
    }

    pub async fn apply_actions(&mut self, actions: Vec<Action>) -> Result<(), NeonError> {
        info!("apply_actions");

        let rent = Rent::get()?;

        let mut new_balance_accounts = HashSet::new();

        for action in actions {
            #[allow(clippy::match_same_arms)]
            match action {
                Action::Transfer {
                    source,
                    target,
                    chain_id,
                    value,
                } => {
                    info!("neon transfer {value} from {source} to {target}");

                    self.use_balance_account(source, chain_id, true).await?;

                    let (key, target) = self.use_balance_account(target, chain_id, true).await?;
                    if target.is_none() {
                        new_balance_accounts.insert(key);
                    }
                }
                Action::Burn {
                    source,
                    value,
                    chain_id,
                } => {
                    info!("neon withdraw {value} from {source}");

                    self.use_balance_account(source, chain_id, true).await?;
                }
                Action::EvmSetStorage {
                    address,
                    index,
                    value,
                } => {
                    info!("set storage {address} -> {index} = {}", hex::encode(value));

                    if index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
                        self.use_contract_account(address, true).await?;
                    } else {
                        let index = index & !U256::new(0xFF);
                        let (_, account) = self.use_storage_cell(address, index, true).await?;

                        let cell_size = StorageCell::required_account_size(1);
                        let empty_size = StorageCell::required_account_size(0);

                        let gas = if account.is_none() {
                            rent.minimum_balance(cell_size)
                        } else {
                            let existing_value = self.storage(address, index).await;
                            if existing_value == [0_u8; 32] {
                                rent.minimum_balance(cell_size)
                                    .saturating_sub(rent.minimum_balance(empty_size))
                            } else {
                                0
                            }
                        };

                        self.gas = self.gas.saturating_add(gas);
                    }
                }
                Action::EvmIncrementNonce { address, chain_id } => {
                    info!("nonce increment {address}");

                    let (key, account) = self.use_balance_account(address, chain_id, true).await?;
                    if account.is_none() {
                        new_balance_accounts.insert(key);
                    }
                }
                Action::EvmSetCode {
                    address,
                    code,
                    chain_id: _,
                } => {
                    info!("set code {address} -> {} bytes", code.len());
                    self.use_contract_account(address, true).await?;

                    let space = ContractAccount::required_account_size(&code);
                    self.gas = self.gas.saturating_add(rent.minimum_balance(space));
                }
                Action::EvmSelfDestruct { address } => {
                    info!("selfdestruct {address}");
                }
                Action::ExternalInstruction {
                    program_id,
                    accounts,
                    fee,
                    ..
                } => {
                    info!("external call {program_id}");

                    self.use_account(program_id, false).await?;

                    for account in accounts {
                        self.use_account(account.pubkey, account.is_writable)
                            .await?;
                    }

                    self.gas = self.gas.saturating_add(fee);
                }
            }
        }

        self.gas = self.gas.saturating_add(
            rent.minimum_balance(BalanceAccount::required_account_size())
                .saturating_mul(new_balance_accounts.len() as u64),
        );

        Ok(())
    }

    async fn ethereum_balance_map_or<F, R>(
        &self,
        address: Address,
        chain_id: u64,
        default: R,
        f: F,
    ) -> R
    where
        F: FnOnce(BalanceAccount) -> R,
    {
        let (pubkey, mut account) = self
            .use_balance_account(address, chain_id, false)
            .await
            .unwrap();

        if let Some(account_data) = &mut account {
            let info = account_info(&pubkey, account_data);
            match BalanceAccount::from_account(self.program_id(), info, Some(address)) {
                Ok(a) => f(a),
                Err(_) => default,
            }
        } else {
            default
        }
    }

    async fn ethereum_contract_map_or<F, R>(&self, address: Address, default: R, f: F) -> R
    where
        F: FnOnce(ContractAccount) -> R,
    {
        let (pubkey, mut account) = self.use_contract_account(address, false).await.unwrap();

        if let Some(account_data) = &mut account {
            let info = account_info(&pubkey, account_data);
            match ContractAccount::from_account(self.program_id(), info) {
                Ok(a) => f(a),
                Err(_) => default,
            }
        } else {
            default
        }
    }

    fn account_override<F, R>(&self, address: Address, f: F) -> Option<R>
    where
        F: FnOnce(&AccountOverride) -> Option<R>,
    {
        self.state_overrides
            .as_ref()
            .and_then(|a| a.get(&address))
            .and_then(f)
    }
}

#[async_trait(? Send)]
impl<'a> AccountStorage for EmulatorAccountStorage<'a> {
    fn program_id(&self) -> &Pubkey {
        debug!("program_id");
        &self.program_id
    }

    fn operator(&self) -> Pubkey {
        info!("operator");
        FAKE_OPERATOR
    }

    fn block_number(&self) -> U256 {
        info!("block_number");
        self.block_number.into()
    }

    fn block_timestamp(&self) -> U256 {
        info!("block_timestamp");
        self.block_timestamp.try_into().unwrap()
    }

    async fn block_hash(&self, slot: u64) -> [u8; 32] {
        info!("block_hash {slot}");

        if let Ok(Some(slot_hashes_account)) = self.use_account(slot_hashes::ID, false).await {
            let slot_hashes_data = slot_hashes_account.data.as_slice();
            find_slot_hash(slot, slot_hashes_data)
        } else {
            panic!("Error querying account {} from Solana", slot_hashes::ID)
        }
    }

    async fn nonce(&self, address: Address, chain_id: u64) -> u64 {
        info!("nonce {address}  {chain_id}");

        let nonce_override = self.account_override(address, |a| a.nonce);
        if let Some(nonce_override) = nonce_override {
            return nonce_override;
        }

        self.ethereum_balance_map_or(address, chain_id, 0_u64, |a| a.nonce())
            .await
    }

    async fn balance(&self, address: Address, chain_id: u64) -> U256 {
        info!("balance {address} {chain_id}");

        let balance_override = self.account_override(address, |a| a.balance);
        if let Some(balance_override) = balance_override {
            return balance_override;
        }

        self.ethereum_balance_map_or(address, chain_id, U256::ZERO, |a| a.balance())
            .await
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
        use evm_loader::error::Error;

        let error = Err(Error::Custom(std::format!(
            "Account {address} - invalid tag"
        )));
        self.ethereum_contract_map_or(address, error, |a| Ok(a.chain_id()))
            .await
    }

    fn contract_pubkey(&self, address: Address) -> (Pubkey, u8) {
        address.find_solana_address(self.program_id())
    }

    async fn code_hash(&self, address: Address, chain_id: u64) -> [u8; 32] {
        use solana_sdk::keccak::hash;

        info!("code_hash {address} {chain_id}");

        let code = self.code(address).await.to_vec();
        if !code.is_empty() {
            return hash(&code).to_bytes();
        }

        // https://eips.ethereum.org/EIPS/eip-1052
        // https://eips.ethereum.org/EIPS/eip-161
        if (self.balance(address, chain_id).await == 0)
            && (self.nonce(address, chain_id).await == 0)
        {
            return <[u8; 32]>::default();
        }

        hash(&[]).to_bytes()
    }

    async fn code_size(&self, address: Address) -> usize {
        info!("code_size {address}");

        self.code(address).await.len()
    }

    async fn code(&self, address: Address) -> evm_loader::evm::Buffer {
        use evm_loader::evm::Buffer;

        info!("code {address}");

        let code_override = self.account_override(address, |a| a.code.clone());
        if let Some(code_override) = code_override {
            return Buffer::from_vec(code_override.into());
        }

        let code = self
            .ethereum_contract_map_or(address, vec![], |c| c.code().to_vec())
            .await;
        Buffer::from_vec(code)
    }

    async fn storage(&self, address: Address, index: U256) -> [u8; 32] {
        let storage_override = self.account_override(address, |a| a.storage(index));
        if let Some(storage_override) = storage_override {
            return storage_override;
        }

        let value = if index < U256::from(STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT as u64) {
            let index: usize = index.as_usize();
            self.ethereum_contract_map_or(address, [0_u8; 32], |c| c.storage_value(index))
                .await
        } else {
            let subindex = (index & 0xFF).as_u8();
            let index = index & !U256::new(0xFF);

            let (pubkey, account) = self.use_storage_cell(address, index, false).await.unwrap();
            if let Some(mut account) = account {
                let account_info = account_info(&pubkey, &mut account);
                StorageCell::from_account(self.program_id(), account_info)
                    .map(|c| c.get(subindex))
                    .unwrap_or_default()
            } else {
                <[u8; 32]>::default()
            }
        };

        info!("storage {address} -> {index} = {}", hex::encode(value));

        value
    }

    async fn clone_solana_account(&self, address: &Pubkey) -> OwnedAccountInfo {
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
            let mut account = self
                .use_account(*address, false)
                .await
                .unwrap_or_default()
                .unwrap_or_default();

            let info = account_info(address, &mut account);
            OwnedAccountInfo::from_account_info(self.program_id(), &info)
        }
    }

    async fn map_solana_account<F, R>(&self, address: &Pubkey, action: F) -> R
    where
        F: FnOnce(&AccountInfo) -> R,
    {
        let mut account = self
            .use_account(*address, false)
            .await
            .unwrap_or_default()
            .unwrap_or_default();

        let info = account_info(address, &mut account);
        action(&info)
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
