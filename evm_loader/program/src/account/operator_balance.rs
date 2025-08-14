use std::cell::{Ref, RefMut};
use std::mem::size_of;

use crate::account::pda;
use crate::{
    error::{Error, Result},
    types::Address,
};
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent};

use super::{
    program, Account, AccountHeader, AccountRead, AccountWrite, Balance, Operator,
    ACCOUNT_PREFIX_LEN, TAG_OPERATOR_BALANCE,
};

#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub address: Address,
    pub chain_id: u64,
    pub balance: U256,
}
impl AccountHeader for Header {
    const VERSION: u8 = 2;
}

#[derive(Clone)]
pub struct OperatorBalance<T> {
    account: T,
}

impl OperatorBalance<()> {
    #[must_use]
    pub const fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>()
    }
}

impl<'a> OperatorBalance<AccountInfo<'a>> {
    pub fn from_account_info(program_id: &Pubkey, account: &AccountInfo<'a>) -> Result<Self> {
        let account = account.clone();
        Self::from_account(program_id, account)
    }

    pub fn try_from_account_info(
        program_id: &Pubkey,
        account: &AccountInfo<'a>,
    ) -> Result<Option<Self>> {
        if account.is_system_owned() {
            Ok(None)
        } else {
            let balance = Self::from_account_info(program_id, account)?;
            Ok(Some(balance))
        }
    }

    pub fn create(
        address: Address,
        chain_id: u64,
        mut account: AccountInfo<'a>,
        operator: &Operator<'a>,
        system: &program::System<'a>,
        rent: &Rent,
    ) -> Result<Self> {
        let (pubkey, bump_seed) = address.find_operator_address(&crate::ID, chain_id, operator);

        if account.key != &pubkey {
            return Err(Error::AccountInvalidKey(*account.key, pubkey));
        }

        // Already created. Return immediately
        if !account.is_system_owned() {
            let balance_account = Self::from_account(&crate::ID, account)?;
            assert_eq!(balance_account.address(), address);
            assert_eq!(balance_account.chain_id(), chain_id);
            assert_eq!(balance_account.owner(), *operator.key);

            return Ok(balance_account);
        }

        // Create a new account
        let seeds = pda::operator_balance_seeds!(operator.key, address, chain_id, bump_seed);
        system.create_pda_account(
            &crate::ID,
            operator,
            &account,
            seeds,
            OperatorBalance::required_account_size(),
            rent,
        )?;

        account.write_tag(TAG_OPERATOR_BALANCE, Header::VERSION)?;
        account.write_header(Header {
            owner: *operator.key,
            address,
            chain_id,
            balance: U256::ZERO,
        });

        Ok(Self { account })
    }

    /// # Safety
    /// Permanently deletes Operator Balance account and all data in it
    pub unsafe fn suicide(self, operator: &Operator) -> Result<()> {
        assert_eq!(self.balance(), U256::ZERO);

        let info = self.account;
        crate::account::delete(&info, operator)
    }
}

impl<T: AccountRead> OperatorBalance<T> {
    pub fn from_account(program_id: &Pubkey, account: T) -> Result<Self> {
        account.validate_tag(program_id, TAG_OPERATOR_BALANCE)?;

        Ok(Self { account })
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    #[must_use]
    pub fn address(&self) -> Address {
        let header: Ref<Header> = self.account.header();
        header.address
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        let header: Ref<Header> = self.account.header();
        header.chain_id
    }

    #[must_use]
    pub fn balance(&self) -> U256 {
        let header: Ref<Header> = self.account.header();
        header.balance
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header: Ref<Header> = self.account.header();
        header.owner
    }

    pub fn validate_owner(&self, operator: &Operator) -> Result<()> {
        let owner = self.owner();
        if &owner != operator.key {
            return Err(Error::OperatorBalanceInvalidOwner(owner, *operator.key));
        }

        Ok(())
    }
}

impl<T: Account> OperatorBalance<T> {
    pub fn consume_gas(&mut self, source: &mut Balance<impl Account>, value: U256) -> Result<()> {
        if self.chain_id() != source.chain_id() {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        source.burn(value)?;
        self.mint(value)
    }

    pub fn withdraw(&mut self, target: &mut Balance<impl Account>) -> Result<()> {
        if self.chain_id() != target.chain_id() {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        if self.address() != target.address() {
            return Err(Error::OperatorBalanceInvalidAddress);
        }

        let value = self.balance();

        self.burn(value)?;
        target.mint(value)
    }

    pub fn burn(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<Header> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_sub(value)
            .ok_or(Error::InsufficientBalance(
                header.address,
                header.chain_id,
                value,
            ))?;

        Ok(())
    }

    pub fn mint(&mut self, value: U256) -> Result<()> {
        let mut header: RefMut<Header> = self.account.header_mut();

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }
}

pub trait OperatorBalanceValidator {
    fn validate(&self, operator: &Operator, chain_id: u64) -> Result<()> {
        self.validate_owner(operator)?;
        self.validate_chain_id(chain_id)
    }

    fn validate_owner(&self, operator: &Operator) -> Result<()>;
    fn validate_chain_id(&self, chain_id: u64) -> Result<()>;

    fn miner(&self, origin: Address) -> Address;
}

impl<T: AccountRead> OperatorBalanceValidator for Option<OperatorBalance<T>> {
    fn validate_owner(&self, operator: &Operator) -> Result<()> {
        let Some(balance) = self else { return Ok(()) };
        balance.validate_owner(operator)
    }

    fn validate_chain_id(&self, chain_id: u64) -> Result<()> {
        let Some(balance) = self else { return Ok(()) };
        if balance.chain_id() != chain_id {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        Ok(())
    }

    fn miner(&self, origin: Address) -> Address {
        self.as_ref().map_or(origin, OperatorBalance::address)
    }
}
