// Mostly copy paste from https://github.com/anza-xyz/pinocchio/blob/main/programs/token/src/instructions

use std::mem::MaybeUninit;

use enum_dispatch::enum_dispatch;
use maybe_async::maybe_async;
use pinocchio_token_interface::native_mint::is_native_mint;
use pinocchio_token_interface::state::{self, load, load_mut, load_mut_unchecked, Initializable};
use solana_program::rent::Rent;
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

use crate::platform::FAKE_OPERATOR;
use crate::{
    error::{Error, Result},
    platform::{InvokeMode, Platform},
    types::seeds::{Seeds, SeedsRef},
};

use super::{AccountsMap, Invokable};

pub const SPL_TOKEN_ID: Pubkey = Pubkey::new_from_array(pinocchio_token_interface::program::ID);

#[enum_dispatch]
pub enum SplToken {
    InitializeMint,
    InitializeAccount,
    CloseAccount,
    Approve,
    Revoke,
    Transfer,
    MintTo,
    Burn,
    Freeze,
    Thaw,
}

pub struct InitializeMint {
    pub account: Pubkey,
    pub decimals: u8,
    pub mint_authority: Pubkey,
    pub freeze_authority: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for InitializeMint {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let account = accounts.get_mut(&self.account).unwrap();
        if account.owner != SPL_TOKEN_ID {
            return Err(Error::AccountInvalidOwner(self.account, SPL_TOKEN_ID));
        }

        let mint = unsafe { load_mut_unchecked::<state::mint::Mint>(&mut account.data) }?;
        if mint.is_initialized()? {
            return Err(Error::AccountAlreadyInitialized(self.account));
        }

        mint.decimals = self.decimals;
        mint.set_supply(0);
        mint.set_mint_authority(&self.mint_authority.to_bytes());
        mint.set_freeze_authority(&self.freeze_authority.to_bytes());
        mint.set_initialized();

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [AccountMeta::new(self.account, false)];

        let mut data = [UNINIT_BYTE; 67];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[20]);
        // Set decimals as u8 at offset [1]
        write_bytes(&mut data[1..2], &[self.decimals]);
        // Set mint_authority as Pubkey at offset [2..34]
        write_bytes(&mut data[2..34], self.mint_authority.as_ref());
        // Set Option = `true` & freeze_authority at offset [34..67]
        write_bytes(&mut data[34..35], &[1]);
        write_bytes(&mut data[35..], self.freeze_authority.as_ref());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        platform.invoke(instruction, &[], InvokeMode::Queued).await
    }
}

pub struct InitializeAccount {
    pub account: Pubkey,
    pub mint: Pubkey,
    pub owner: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for InitializeAccount {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, platform: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let account = accounts.get_mut(&self.account).unwrap();
        if account.owner != SPL_TOKEN_ID {
            return Err(Error::AccountInvalidOwner(self.account, SPL_TOKEN_ID));
        }

        let data_len = account.data.len();

        let token = unsafe { load_mut_unchecked::<state::account::Account>(&mut account.data) }?;
        if token.is_initialized()? {
            return Err(Error::AccountAlreadyInitialized(self.account));
        }

        token.mint = self.mint.to_bytes();
        token.owner = self.owner.to_bytes();
        if is_native_mint(&token.mint) {
            let rent: Rent = platform.get_sysvar().await?;
            let native_amount = rent.minimum_balance(data_len);
            let amount = account.lamports.saturating_sub(native_amount);

            token.set_native(true);
            token.set_native_amount(native_amount);
            token.set_amount(amount);
        }

        token.set_account_state(state::account_state::AccountState::Initialized);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.mint, false),
        ];

        let mut data = [UNINIT_BYTE; 33];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[18]);
        // Set owner as [u8; 32] at offset [1..33]
        write_bytes(&mut data[1..], self.owner.as_ref());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        platform.invoke(instruction, &[], InvokeMode::Queued).await
    }
}

pub struct CloseAccount {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for CloseAccount {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let account = accounts.get_mut(&self.account).unwrap();
        if account.owner != SPL_TOKEN_ID {
            return Err(Error::AccountInvalidOwner(self.account, SPL_TOKEN_ID));
        }

        let token = unsafe { load::<state::account::Account>(&account.data) }?;
        if !token.is_native() && token.amount() != 0 {
            return Err("Non-native account can only be closed if its balance is zero".into());
        }

        account.owner = solana_program::system_program::id();
        account.data.clear();
        account.lamports = 0;

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new(FAKE_OPERATOR, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: vec![9],
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Approve {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
    pub delegate: Pubkey,
    pub amount: u64,
}

#[maybe_async(?Send)]
impl Invokable for Approve {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let token = load_account_mut(&self.account, accounts)?;
        token.set_delegate(&self.delegate.to_bytes());
        token.set_delegated_amount(self.amount);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.delegate, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let mut data = [UNINIT_BYTE; 9];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[4]);
        // Set amount as u64 at offset [1..9]
        write_bytes(&mut data[1..], &self.amount.to_le_bytes());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Revoke {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for Revoke {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let token = load_account_mut(&self.account, accounts)?;
        token.clear_delegate();
        token.set_delegated_amount(0);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: vec![5],
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Transfer {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub source: Pubkey,
    pub target: Pubkey,
    pub amount: u64,
}

#[maybe_async(?Send)]
impl Invokable for Transfer {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.source);
        f(&self.target);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        // Subtract amount from source account
        let source = load_account_mut(&self.source, accounts)?;
        let Some(amount) = source.amount().checked_sub(self.amount) else {
            return Err("insufficient funds".into());
        };
        source.set_amount(amount);

        // Add amount to target account
        let target = load_account_mut(&self.target, accounts)?;
        target.set_amount(target.amount() + self.amount); // won't overflow

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.source, false),
            AccountMeta::new(self.target, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let mut data = [UNINIT_BYTE; 9];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[3]);
        // Set amount as u64 at offset [1..9]
        write_bytes(&mut data[1..9], &self.amount.to_le_bytes());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct MintTo {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
}

#[maybe_async(?Send)]
impl Invokable for MintTo {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
        f(&self.mint);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        // Increase supply
        let mint = load_mint_mut(&self.mint, accounts)?;

        let Some(supply) = mint.supply().checked_add(self.amount) else {
            return Err("Operation overflowed".into());
        };
        mint.set_supply(supply);

        // Mint tokens
        let token = load_account_mut(&self.account, accounts)?;
        token.set_amount(token.amount() + self.amount); // won't overflow

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.mint, false),
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let mut data = [UNINIT_BYTE; 9];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[7]);
        // Set amount as u64 at offset [1..9]
        write_bytes(&mut data[1..9], &self.amount.to_le_bytes());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Burn {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
}

#[maybe_async(?Send)]
impl Invokable for Burn {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
        f(&self.mint);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        // Burn tokens
        let token = load_account_mut(&self.account, accounts)?;

        let Some(amount) = token.amount().checked_sub(self.amount) else {
            return Err("Operation overflowed".into());
        };
        token.set_amount(amount);

        // Decrease supply
        let mint = load_mint_mut(&self.mint, accounts)?;
        mint.set_supply(mint.supply() - self.amount); // won't overflow

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new(self.mint, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let mut data = [UNINIT_BYTE; 9];
        // Set discriminator as u8 at offset [0]
        write_bytes(&mut data, &[8]);
        // Set amount as u64 at offset [1..9]
        write_bytes(&mut data[1..9], &self.amount.to_le_bytes());

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: unsafe { assume_init_to_vec(data) },
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Freeze {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
    pub mint: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for Freeze {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let token = load_account_mut(&self.account, accounts)?;
        token.set_account_state(state::account_state::AccountState::Frozen);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: vec![10],
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

pub struct Thaw {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub account: Pubkey,
    pub mint: Pubkey,
}

#[maybe_async(?Send)]
impl Invokable for Thaw {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.account);
    }

    async fn emulate(&self, _: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let token = load_account_mut(&self.account, accounts)?;
        token.set_account_state(state::account_state::AccountState::Initialized);

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let accounts = [
            AccountMeta::new(self.account, false),
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(self.authority, true),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: SPL_TOKEN_ID,
            accounts: accounts.to_vec(),
            data: vec![11],
        };

        let seeds = SeedsRef::new(&self.seeds);
        let seeds = seeds.as_slices();

        platform
            .invoke(instruction, &[seeds], InvokeMode::Queued)
            .await
    }
}

const UNINIT_BYTE: MaybeUninit<u8> = MaybeUninit::<u8>::uninit();

#[inline(always)]
fn write_bytes(destination: &mut [MaybeUninit<u8>], source: &[u8]) {
    for (d, s) in destination.iter_mut().zip(source.iter()) {
        d.write(*s);
    }
}

#[inline(always)]
unsafe fn assume_init_to_vec<const N: usize>(data: [MaybeUninit<u8>; N]) -> Vec<u8> {
    let ptr = data.as_ptr().cast::<u8>();
    unsafe { std::slice::from_raw_parts(ptr, N) }.to_vec()
}

#[inline(always)]
fn load_account_mut<'a>(
    pubkey: &Pubkey,
    accounts: &'a mut AccountsMap,
) -> Result<&'a mut state::account::Account> {
    let account = accounts.get_mut(pubkey).unwrap();
    if account.owner != SPL_TOKEN_ID {
        return Err(Error::AccountInvalidOwner(*pubkey, SPL_TOKEN_ID));
    }

    let token = unsafe { load_mut::<state::account::Account>(&mut account.data) }?;
    Ok(token)
}

#[inline(always)]
fn load_mint_mut<'a>(
    pubkey: &Pubkey,
    accounts: &'a mut AccountsMap,
) -> Result<&'a mut state::mint::Mint> {
    let account = accounts.get_mut(pubkey).unwrap();
    if account.owner != SPL_TOKEN_ID {
        return Err(Error::AccountInvalidOwner(*pubkey, SPL_TOKEN_ID));
    }

    let token = unsafe { load_mut::<state::mint::Mint>(&mut account.data) }?;
    Ok(token)
}
