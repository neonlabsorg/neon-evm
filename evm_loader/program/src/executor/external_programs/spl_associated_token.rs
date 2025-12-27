use enum_dispatch::enum_dispatch;
use maybe_async::maybe_async;
use pinocchio_token_interface::state::{self, Transmutable};
use solana_program::{
    instruction::AccountMeta,
    pubkey::{pubkey, Pubkey},
};

use crate::{
    error::Result,
    platform::{InvokeMode, Platform, FAKE_OPERATOR},
    types::seeds::Seeds,
};

use super::{spl_token::SPL_TOKEN_ID, AccountsMap, Invokable};

pub const SPL_ASSOCIATED_TOKEN_ID: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

#[enum_dispatch]
pub enum SplAssociatedToken {
    CreateIdempotent(CreateAssociatedToken),
}

#[must_use]
pub fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            &wallet.to_bytes(),
            &SPL_TOKEN_ID.to_bytes(),
            &mint.to_bytes(),
        ],
        &SPL_ASSOCIATED_TOKEN_ID,
    )
    .0
}

pub struct CreateAssociatedToken {
    pub account: Pubkey,
    pub owner: Pubkey,
    pub mint: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for CreateAssociatedToken {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, platform: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let account = accounts.get(&self.account).unwrap();
        if account.owner == SPL_TOKEN_ID {
            return Ok(());
        }

        super::system::CreateAccount {
            account: self.account,
            seeds: Seeds::new(&[]),
            owner: SPL_TOKEN_ID,
            space: state::account::Account::LEN,
        }
        .emulate(platform, accounts)
        .await?;

        super::spl_token::InitializeAccount {
            account: self.account,
            mint: self.mint,
            owner: self.owner,
        }
        .emulate(platform, accounts)
        .await
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(FAKE_OPERATOR, true),
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.owner, false),
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_ID, false),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_ASSOCIATED_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: vec![1], // CreateIdempotent
        };

        platform.invoke(instruction, &[], InvokeMode::Queued).await
    }
}
