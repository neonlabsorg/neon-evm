use crate::account::AccountRead;
use crate::error::{Error, Result};
use crate::executor::external_programs::spl_token::SPL_TOKEN_ID;
use pinocchio_token_interface::state::{load, load_unchecked, Initializable, Transmutable};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

use std::cell::Ref;
use std::marker::PhantomData;

pub struct Account<'a, T: Initializable + Transmutable> {
    pub info: AccountInfo<'a>,
    phantom: PhantomData<&'a T>,
}

impl<'a, T: Initializable + Transmutable> Account<'a, T> {
    pub fn from_account_info(info: &AccountInfo<'a>) -> Result<Self> {
        if info.owner() != SPL_TOKEN_ID {
            return Err(Error::AccountInvalidOwner(info.pubkey(), SPL_TOKEN_ID));
        }

        let data = info.try_borrow_data()?;
        let _ = unsafe { load::<T>(&data) }?;

        Ok(Self {
            info: info.clone(),
            phantom: PhantomData,
        })
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        *self.info.key
    }

    #[must_use]
    pub fn load(&self) -> Ref<T> {
        let data = self.info.data();
        Ref::map(data, |d| unsafe { load_unchecked(d).unwrap() })
    }
}

pub type State<'a> = Account<'a, pinocchio_token_interface::state::account::Account>;
pub type Mint<'a> = Account<'a, pinocchio_token_interface::state::mint::Mint>;

impl State<'_> {
    #[must_use]
    pub fn mint(&self) -> Pubkey {
        let mint = self.load().mint;
        Pubkey::new_from_array(mint)
    }

    #[must_use]
    pub fn amount(&self) -> u64 {
        self.load().amount()
    }

    #[must_use]
    pub fn delegated_amount(&self) -> u64 {
        self.load().delegated_amount()
    }

    #[must_use]
    pub fn delegate(&self) -> Option<&Pubkey> {
        #[allow(clippy::transmute_ptr_to_ptr)]
        self.load()
            .delegate()
            .map(|d| unsafe { std::mem::transmute(d) })
    }
}

impl Mint<'_> {
    #[must_use]
    pub fn decimals(&self) -> u8 {
        self.load().decimals
    }
}
