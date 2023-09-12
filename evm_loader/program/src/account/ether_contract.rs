use crate::{
    account::TAG_EMPTY,
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use serde::Deserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::MAX_PERMITTED_DATA_INCREASE, pubkey::Pubkey, rent::Rent,
    system_program,
};
use std::{
    cell::{Ref, RefMut},
    mem::size_of,
};

use crate::config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;

use super::{AccountsDB, ACCOUNT_PREFIX_LEN, ACCOUNT_SEED_VERSION, TAG_ACCOUNT_CONTRACT};

#[deprecated]
#[derive(Deserialize)]
pub struct DataV3 {
    /// Ethereum address
    pub address: Address,
    /// Solana account nonce
    pub bump_seed: u8,
    /// Ethereum account nonce
    pub trx_count: u64,
    /// Neon token balance
    #[serde(with = "ethnum::serde::bytes::le")]
    pub balance: U256,
    /// Account generation, increment on suicide
    pub generation: u32,
    /// Contract code size
    pub code_size: u32,
    /// Read-write lock
    pub rw_blocked: bool,
}

#[derive(Eq, PartialEq)]
pub enum AllocateResult {
    Ready,
    NeedMore,
}

#[repr(C, packed)]
pub struct Header {
    pub chain_id: u64,
    pub generation: u32,
}

pub type Storage = [[u8; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT];
pub type Code = [u8];

pub struct ContractAccount<'a> {
    account: AccountInfo<'a>,
}

const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;
const STORAGE_OFFSET: usize = HEADER_OFFSET + size_of::<Header>();
const CODE_OFFSET: usize = STORAGE_OFFSET + size_of::<Storage>();

impl<'a> ContractAccount<'a> {
    pub fn required_account_size(code: &[u8]) -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>() + size_of::<Storage>() + code.len()
    }

    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, &account, TAG_ACCOUNT_CONTRACT)?;

        Ok(Self { account })
    }

    pub fn allocate(
        address: &Address,
        code: &[u8],
        rent: &Rent,
        accounts: &AccountsDB,
    ) -> Result<AllocateResult> {
        let (pubkey, bump_seed) = address.find_solana_address(&crate::ID);
        let info = accounts.get(&pubkey);

        let required_size = Self::required_account_size(code);
        if info.data_len() >= required_size {
            return Ok(AllocateResult::Ready);
        }

        let system = accounts.system();
        let operator = accounts.operator();

        if system_program::check_id(info.owner) {
            let seeds: &[&[u8]] = &[&[ACCOUNT_SEED_VERSION], address.as_bytes(), &[bump_seed]];
            let space = required_size.min(MAX_PERMITTED_DATA_INCREASE);
            system.create_pda_account(&crate::ID, operator, info, seeds, space)?;
        } else if crate::check_id(info.owner) {
            super::validate_tag(&crate::ID, info, TAG_EMPTY)?;

            let max_size = info.data_len() + MAX_PERMITTED_DATA_INCREASE;
            let space = required_size.min(max_size);
            info.realloc(space, false)?;

            let required_balance = rent.minimum_balance(space);
            if info.lamports() < required_balance {
                let lamports = required_balance - info.lamports();
                system.transfer(operator, info, lamports)?;
            }
        } else {
            return Err(Error::AccountInvalidOwner(pubkey, system_program::ID));
        }

        if info.data_len() >= required_size {
            Ok(AllocateResult::Ready)
        } else {
            Ok(AllocateResult::NeedMore)
        }
    }

    pub fn init(
        address: &Address,
        chain_id: u64,
        code: &[u8],
        accounts: &AccountsDB<'a>,
    ) -> Result<Self> {
        let (pubkey, _) = address.find_solana_address(&crate::ID);
        let account = accounts.get(&pubkey).clone();

        super::validate_tag(&crate::ID, &account, TAG_EMPTY)?;
        super::set_tag(&crate::ID, &account, TAG_ACCOUNT_CONTRACT)?;

        let mut contract = Self::from_account(&crate::ID, account)?;
        {
            let mut header = contract.header_mut();
            header.chain_id = chain_id;
            header.generation = 0;
        }
        {
            let mut contract_code = contract.code_mut();
            contract_code.copy_from_slice(code);
        }

        Ok(contract)
    }

    pub fn pubkey(&self) -> &'a Pubkey {
        self.account.key
    }

    #[inline]
    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    #[inline]
    fn storage(&self) -> Ref<Storage> {
        super::section(&self.account, STORAGE_OFFSET)
    }

    #[inline]
    fn storage_mut(&mut self) -> RefMut<Storage> {
        super::section_mut(&self.account, STORAGE_OFFSET)
    }

    #[inline]
    pub fn code(&self) -> Ref<Code> {
        let data = self.account.data.borrow();
        Ref::map(data, |d| &d[CODE_OFFSET..])
    }

    #[inline]
    fn code_mut(&self) -> RefMut<Code> {
        let data = self.account.data.borrow_mut();
        RefMut::map(data, |d| &mut d[CODE_OFFSET..])
    }

    pub fn code_buffer(&self) -> crate::evm::Buffer {
        let begin = CODE_OFFSET;
        let end = begin + self.code_len();

        unsafe { crate::evm::Buffer::from_account(&self.account, begin..end) }
    }

    pub fn code_len(&self) -> usize {
        self.account.data_len().saturating_sub(CODE_OFFSET)
    }

    pub fn chain_id(&self) -> u64 {
        self.header().chain_id
    }

    pub fn generation(&self) -> u32 {
        self.header().generation
    }

    pub fn storage_value(&self, index: usize) -> [u8; 32] {
        assert!(index < STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT);

        let storage = self.storage();
        storage[index]
    }

    pub fn set_storage_value(&mut self, index: usize, value: &[u8; 32]) {
        assert!(index < STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT);

        let mut storage = self.storage_mut();

        let cell: &mut [u8; 32] = &mut storage[index];
        cell.copy_from_slice(value);
    }
}
