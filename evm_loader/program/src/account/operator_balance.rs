use std::mem::size_of;

use crate::pda_seeds::with_operator_seeds;
use crate::{
    error::{Error, Result},
    types::{Address, Transaction},
};
use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey, rent::Rent, system_program};

use super::{
    program, AccountHeader, BalanceAccount, Operator, ACCOUNT_PREFIX_LEN, TAG_OPERATOR_BALANCE,
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
pub struct OperatorBalanceAccount<'a> {
    account: &'a AccountInfo<'a>,
}

impl<'a> OperatorBalanceAccount<'a> {
    #[must_use]
    pub fn required_account_size() -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>()
    }

    pub fn from_account(program_id: &Pubkey, account: &'a AccountInfo<'a>) -> Result<Self> {
        super::validate_tag(program_id, account, TAG_OPERATOR_BALANCE)?;

        Ok(Self { account })
    }

    pub fn try_from_account(
        program_id: &Pubkey,
        account: &'a AccountInfo<'a>,
    ) -> Result<Option<Self>> {
        if system_program::check_id(account.owner) {
            Ok(None)
        } else {
            let balance = Self::from_account(program_id, account)?;
            Ok(Some(balance))
        }
    }

    pub fn create(
        address: Address,
        chain_id: u64,
        account: &'a AccountInfo<'a>,
        operator: &Operator<'a>,
        system: &program::System<'a>,
        rent: &Rent,
    ) -> Result<Self> {
        let (pubkey, bump_seed) = address.find_operator_address(&crate::ID, chain_id, operator);

        if account.key != &pubkey {
            return Err(Error::AccountInvalidKey(*account.key, pubkey));
        }

        // Already created. Return immidiately
        if !system_program::check_id(account.owner) {
            let balance_account = Self::from_account(&crate::ID, account)?;
            assert_eq!(balance_account.address(), address);
            assert_eq!(balance_account.chain_id(), chain_id);
            assert_eq!(balance_account.owner(), *operator.key);

            return Ok(balance_account);
        }

        // Create a new account
        with_operator_seeds(
            operator,
            &address,
            chain_id,
            &[bump_seed],
            |program_seeds| {
                system.create_pda_account(
                    &crate::ID,
                    operator,
                    account,
                    program_seeds,
                    Self::required_account_size(),
                    rent,
                )
            },
        )?;

        super::set_tag(&crate::ID, account, TAG_OPERATOR_BALANCE, Header::VERSION)?;
        {
            let mut header = super::header_mut::<Header>(account);
            header.owner = *operator.key;
            header.address = address;
            header.chain_id = chain_id;
            header.balance = U256::ZERO;
        }

        Ok(Self { account })
    }

    #[must_use]
    pub fn pubkey(&self) -> &'a Pubkey {
        self.account.key
    }

    #[must_use]
    pub fn address(&self) -> Address {
        let header = super::header::<Header>(self.account);
        header.address
    }

    #[must_use]
    pub fn chain_id(&self) -> u64 {
        let header = super::header::<Header>(self.account);
        header.chain_id
    }

    #[must_use]
    pub fn balance(&self) -> U256 {
        let header = super::header::<Header>(self.account);
        header.balance
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header = super::header::<Header>(self.account);
        header.owner
    }

    pub fn validate_owner(&self, operator: &Operator) -> Result<()> {
        let owner = self.owner();
        if &owner != operator.key {
            return Err(Error::OperatorBalanceInvalidOwner(owner, *operator.key));
        }

        Ok(())
    }

    pub fn consume_gas(&mut self, source: &mut BalanceAccount, value: U256) -> Result<()> {
        if self.chain_id() != source.chain_id() {
            return Err(Error::OperatorBalanceInvalidChainId);
        }

        source.burn(value)?;
        self.mint(value)
    }

    pub fn withdraw(&mut self, target: &mut BalanceAccount) -> Result<()> {
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
        let mut header = super::header_mut::<Header>(self.account);

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
        let mut header = super::header_mut::<Header>(self.account);

        header.balance = header
            .balance
            .checked_add(value)
            .ok_or(Error::IntegerOverflow)?;

        Ok(())
    }

    /// # Safety
    /// Permanently deletes Operator Balance account and all data in it
    pub unsafe fn suicide(self, operator: &Operator) {
        assert_eq!(self.balance(), U256::ZERO);

        crate::account::delete(self.account, operator);
    }
}

pub trait OperatorBalanceValidator {
    fn validate(&self, operator: &Operator, trx: &Transaction) -> Result<()> {
        self.validate_owner(operator)?;
        self.validate_transaction(trx)
    }

    fn validate_owner(&self, operator: &Operator) -> Result<()>;
    fn validate_transaction(&self, trx: &Transaction) -> Result<()>;

    fn miner(&self, origin: Address) -> Address;
}

impl OperatorBalanceValidator for Option<OperatorBalanceAccount<'_>> {
    fn validate_owner(&self, operator: &Operator) -> Result<()> {
        let Some(balance) = self else { return Ok(()) };
        balance.validate_owner(operator)
    }

    fn validate_transaction(&self, trx: &Transaction) -> Result<()> {
        if self.is_none() && (trx.gas_price() != U256::ZERO) {
            return Err(Error::OperatorBalanceMissing);
        }

        Ok(())
    }

    fn miner(&self, origin: Address) -> Address {
        self.as_ref()
            .map_or(origin, OperatorBalanceAccount::address)
    }
}
