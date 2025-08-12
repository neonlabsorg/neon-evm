use enum_dispatch::enum_dispatch;
use maybe_async::maybe_async;
use solana_program::{instruction::AccountMeta, pubkey::Pubkey, rent::Rent};

use crate::{
    account::AccountRead,
    error::{Error, Result},
    platform::{InvokeMode, Platform, FAKE_OPERATOR},
    types::seeds::{Seeds, SeedsRef},
};

use super::{AccountsMap, Invokable};

#[enum_dispatch]
pub enum SystemProgram {
    CreateAccount(CreateAccount),
}

pub struct CreateAccount {
    pub account: Pubkey,
    pub seeds: Seeds,

    pub owner: Pubkey,
    pub space: usize,
}

#[maybe_async(?Send)]
impl CreateAccount {
    async fn create(&self, platform: &mut impl Platform) -> Result<()> {
        let rent: Rent = platform.get_sysvar().await?;
        let lamports = rent.minimum_balance(self.space);

        let mut data = [0; 52];
        // create account instruction has a '0' discriminator
        data[4..12].copy_from_slice(&lamports.to_le_bytes());
        data[12..20].copy_from_slice(&self.space.to_le_bytes());
        data[20..52].copy_from_slice(self.owner.as_ref());

        let accounts = [
            AccountMeta::new(FAKE_OPERATOR, true),
            AccountMeta::new(self.account, true),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: solana_sdk_ids::system_program::id(),
            accounts: accounts.to_vec(),
            data: data.to_vec(),
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }

    async fn transfer(&self, platform: &mut impl Platform) -> Result<()> {
        let rent: Rent = platform.get_sysvar().await?;
        let minimum_balance = rent.minimum_balance(self.space);

        let lamports = {
            let account = platform.get_raw_account(&self.account).await?;
            minimum_balance.saturating_sub(account.lamports())
        };

        if lamports == 0 {
            return Ok(());
        }

        let mut data = [0; 12];
        data[0] = 2;
        data[4..12].copy_from_slice(&lamports.to_le_bytes());

        let accounts = [
            AccountMeta::new(FAKE_OPERATOR, true),
            AccountMeta::new(self.account, false),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: solana_sdk_ids::system_program::id(),
            accounts: accounts.to_vec(),
            data: data.to_vec(),
        };

        platform.invoke(instruction, &[], InvokeMode::Queued).await
    }

    async fn allocate(&self, platform: &mut impl Platform) -> Result<()> {
        let mut data = [0; 12];
        data[0] = 8;
        data[4..12].copy_from_slice(&self.space.to_le_bytes());

        let accounts = [AccountMeta::new(self.account, true)];

        let instruction = solana_program::instruction::Instruction {
            program_id: solana_sdk_ids::system_program::id(),
            accounts: accounts.to_vec(),
            data: data.to_vec(),
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }

    async fn assign(&self, platform: &mut impl Platform) -> Result<()> {
        let mut data = [0; 36];
        data[0] = 1;
        data[4..36].copy_from_slice(self.owner.as_ref());

        let accounts = [AccountMeta::new(self.account, true)];

        let instruction = solana_program::instruction::Instruction {
            program_id: solana_sdk_ids::system_program::id(),
            accounts: accounts.to_vec(),
            data: data.to_vec(),
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

#[maybe_async(?Send)]
impl Invokable for CreateAccount {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, platform: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let account = accounts.get_mut(&self.account).unwrap();
        if !solana_sdk_ids::system_program::check_id(&account.owner) {
            return Err(Error::AccountAlreadyInitialized(self.account));
        }

        let rent: Rent = platform.get_sysvar().await?;
        let minimum_balance = rent.minimum_balance(self.space);

        account.owner = self.owner;
        account.data.resize(self.space, 0);
        account.lamports = std::cmp::max(minimum_balance, account.lamports);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let can_use_create_account = {
            let account = platform.get_raw_account(&self.account).await?;
            account.lamports() == 0
        };

        if can_use_create_account {
            self.create(platform).await
        } else {
            self.transfer(platform).await?;
            self.allocate(platform).await?;
            self.assign(platform).await
        }
    }
}
