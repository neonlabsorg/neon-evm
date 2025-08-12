use arrayvec::{ArrayString, ArrayVec};
use enum_dispatch::enum_dispatch;
use maybe_async::maybe_async;
use solana_program::{
    instruction::AccountMeta,
    pubkey::{pubkey, Pubkey},
};

use crate::{
    error::Result,
    platform::{InvokeMode, Platform, FAKE_OPERATOR},
    types::seeds::{Seeds, SeedsRef},
};

use super::{AccountsMap, Invokable};

pub const MPL_TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

#[enum_dispatch]
pub enum Metaplex {
    CreateMetadata,
    CreateMasterEdition,
}

#[repr(C, packed)]
pub struct Creator {
    pub address: [u8; 32],
    pub verified: bool,
    pub share: u8,
}

#[repr(C, packed)]
pub struct Collection {
    pub verified: bool,
    pub key: [u8; 32],
}

#[repr(C, packed)]
pub struct Uses {
    pub use_method: u8,
    pub remaining: u64,
    pub total: u64,
}

pub struct Metadata<'a> {
    pub key: u8,
    pub update_authority: &'a [u8; 32],
    pub mint: &'a [u8; 32],
    pub name: &'a [u8],
    pub symbol: &'a [u8],
    pub uri: &'a [u8],
    pub seller_fee_basis_points: &'a [u8; 2],
    pub creators: Option<&'a [Creator]>,
    pub primary_sale_happened: u8,
    pub is_mutable: u8,
    pub edition_nonce: Option<u8>,
    pub rest: &'a [u8],
}

pub struct MasterEdition {
    pub key: u8,
    pub supply: u64,
    pub max_supply: Option<u64>,
}

type Name = ArrayString<32>;
type Symbol = ArrayString<10>;
type Uri = ArrayString<200>;

pub struct CreateMetadata {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub metadata: Pubkey,
    pub mint: Pubkey,

    pub name: Name,
    pub symbol: Symbol,
    pub uri: Uri,
}

pub struct CreateMasterEdition {
    pub authority: Pubkey,
    pub seeds: Seeds,

    pub metadata: Pubkey,
    pub edition: Pubkey,
    pub mint: Pubkey,

    pub max_supply: Option<u64>,
}

impl<'a> Metadata<'a> {
    #[must_use]
    pub fn find_pda(mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"metadata", MPL_TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
            &MPL_TOKEN_METADATA_ID,
        )
    }

    pub fn deserialize(mut data: &'a [u8]) -> Self {
        Metadata {
            key: get_u8(&mut data),
            update_authority: get_array(&mut data),
            mint: get_array(&mut data),
            name: get_slice(&mut data),
            symbol: get_slice(&mut data),
            uri: get_slice(&mut data),
            seller_fee_basis_points: get_array(&mut data),
            creators: get_creators(&mut data),
            primary_sale_happened: get_u8(&mut data),
            is_mutable: get_u8(&mut data),
            edition_nonce: get_option(&mut data, get_u8),
            rest: data,
        }
    }

    pub fn token_standard(&self) -> Option<u8> {
        let mut data = self.rest;

        if data.is_empty() {
            return None;
        }

        get_option(&mut data, get_u8)
    }
}

impl MasterEdition {
    #[must_use]
    pub fn find_pda(mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                b"metadata",
                MPL_TOKEN_METADATA_ID.as_ref(),
                mint.as_ref(),
                b"edition",
            ],
            &MPL_TOKEN_METADATA_ID,
        )
    }
}

impl CreateMetadata {
    /// # Safety
    /// Write at most 329 bytes to the buffer.
    /// Caller responsible to ensure that the buffer is large enough.
    #[inline]
    unsafe fn write_mpl_data_unchecked<const N: usize>(
        &self,
        buffer: &mut ArrayVec<u8, N>,
        platform: &impl Platform,
    ) {
        write_string_unchecked(buffer, &self.name); // 36
        write_string_unchecked(buffer, &self.symbol); // 50
        write_string_unchecked(buffer, &self.uri); // 254

        // seller_fee_basis_points
        buffer.try_extend_from_slice(&[0, 0]).unwrap_unchecked(); // 256

        // creators
        buffer.push_unchecked(1 /*Some*/); // 257

        let len: [u8; 4] = 2u32.to_le_bytes();
        buffer.try_extend_from_slice(&len).unwrap_unchecked(); // 261

        // NeonEVM program as a first creator
        let neon_evm = platform.program_id();
        write_pubkey_unchecked(buffer, neon_evm); // 293
        buffer.try_extend_from_slice(&[0, 0]).unwrap_unchecked(); // 295

        // Contract as a second creator
        write_pubkey_unchecked(buffer, &self.authority); // 327
        buffer.try_extend_from_slice(&[1, 100]).unwrap_unchecked(); // 329
    }

    #[inline]
    fn write_metadata(&self, platform: &impl Platform) -> ArrayVec<u8, 400> {
        let mut buffer = ArrayVec::<u8, 400>::new();

        // SAFETY: Buffer is large enough to hold everything we write.
        unsafe {
            buffer.push_unchecked(4 /*MetadataV1*/); // 1
            write_pubkey_unchecked(&mut buffer, &self.authority); // 33
            write_pubkey_unchecked(&mut buffer, &self.mint); // 65
            self.write_mpl_data_unchecked(&mut buffer, platform); // 394
            buffer.push_unchecked(0); // 395 primary_sale_happened,
            buffer.push_unchecked(1); // 396 is_mutable
            buffer.push_unchecked(0 /*None*/); // 397 edition_nonce
        }

        assert!(buffer.len() <= buffer.capacity());
        buffer
    }

    #[inline]
    fn write_create_metadata_instruction(&self, platform: &impl Platform) -> ArrayVec<u8, 340> {
        let mut buffer = ArrayVec::<u8, 340>::new();

        // SAFETY: Buffer is large enough to hold everything we write.
        unsafe {
            buffer.push_unchecked(33); // 1 descriminator
            self.write_mpl_data_unchecked(&mut buffer, platform); // 330
            buffer.push_unchecked(0); // 331 collection
            buffer.push_unchecked(0); // 332 uses
            buffer.push_unchecked(1); // 333 mutable
            buffer.push_unchecked(0); // 334 collection_details
        }

        assert!(buffer.len() <= buffer.capacity());
        buffer
    }
}

#[maybe_async(?Send)]
impl Invokable for CreateMetadata {
    fn for_each_mutable_account<'a>(&'a self, mut f: impl FnMut(&'a Pubkey)) {
        f(&self.metadata);
    }

    async fn emulate(&self, platform: &impl Platform, accounts: &mut AccountsMap) -> Result<()> {
        let data = self.write_metadata(platform);

        let metadata_account = accounts.get_mut(&self.metadata).unwrap();
        metadata_account.owner = MPL_TOKEN_METADATA_ID;
        metadata_account.data = data.to_vec();

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let data = self.write_create_metadata_instruction(platform);

        let accounts = [
            AccountMeta::new(self.metadata, false),
            AccountMeta::new_readonly(self.mint, false),
            AccountMeta::new_readonly(self.authority, true),
            AccountMeta::new(FAKE_OPERATOR, true),
            AccountMeta::new_readonly(self.authority, true),
            AccountMeta::new_readonly(solana_program::system_program::ID, false),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: MPL_TOKEN_METADATA_ID,
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

impl CreateMasterEdition {
    #[inline]
    fn write_create_master_edition(&self) -> ArrayVec<u8, 10> {
        let mut buffer = ArrayVec::<u8, 10>::new();

        // SAFETY: Buffer is large enough to hold everything we write.
        unsafe {
            buffer.push_unchecked(17); // 1 descriminator
            if let Some(max_supply) = self.max_supply {
                buffer.push_unchecked(1); // 2 Some

                let max_supply: [u8; 8] = max_supply.to_le_bytes();
                buffer.try_extend_from_slice(&max_supply).unwrap_unchecked(); // 10
            } else {
                buffer.push_unchecked(0); // 2 None
            }
        }

        assert!(buffer.len() <= buffer.capacity());
        buffer
    }
}

#[maybe_async(?Send)]
impl Invokable for CreateMasterEdition {
    fn for_each_mutable_account<'a>(&'a self, _: impl FnMut(&'a Pubkey)) {
        //do nothing
    }

    async fn emulate(&self, _: &impl Platform, _: &mut AccountsMap) -> Result<()> {
        // do nothing
        // we don't care about things that could happen here

        Ok(())
    }

    async fn invoke(&self, platform: &mut impl Platform) -> Result<()> {
        let data = self.write_create_master_edition();

        let accounts = [
            AccountMeta::new(self.edition, false),
            AccountMeta::new(self.mint, false),
            AccountMeta::new_readonly(self.authority, true),
            AccountMeta::new_readonly(self.authority, true),
            AccountMeta::new(FAKE_OPERATOR, true),
            AccountMeta::new(self.metadata, false),
            AccountMeta::new_readonly(pinocchio_token_interface::program::ID.into(), false),
            AccountMeta::new_readonly(solana_program::system_program::ID, false),
        ];

        let instruction = solana_program::instruction::Instruction {
            program_id: MPL_TOKEN_METADATA_ID,
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

/// # Safety
/// Write at most 32 bytes to the buffer.
/// Caller responsible to ensure that the buffer is large enough.
#[inline]
unsafe fn write_pubkey_unchecked<const N: usize>(buffer: &mut ArrayVec<u8, N>, pubkey: &Pubkey) {
    let bytes = pubkey.as_ref();
    buffer.try_extend_from_slice(bytes).unwrap_unchecked();
}

/// # Safety
/// Write at most `S + 4` bytes to the buffer.
/// Caller responsible to ensure that the buffer is large enough.
#[inline]
#[allow(clippy::cast_possible_truncation)] // all strings here are very small
unsafe fn write_string_unchecked<const N: usize, const S: usize>(
    buffer: &mut ArrayVec<u8, N>,
    string: &ArrayString<S>,
) {
    let bytes = string.as_bytes();
    let len: [u8; 4] = (string.len() as u32).to_le_bytes();

    buffer.try_extend_from_slice(&len).unwrap_unchecked();
    buffer.try_extend_from_slice(bytes).unwrap_unchecked();
}

#[inline]
fn get_u8(data: &mut &[u8]) -> u8 {
    let value = data[0];
    *data = &data[1..];

    value
}

#[inline]
fn get_len(data: &mut &[u8]) -> usize {
    let len: &[u8; 4] = get_array(data);
    u32::from_le_bytes(*len) as usize
}

#[inline]
fn get_option<'a, R>(data: &mut &'a [u8], getter: impl FnOnce(&mut &'a [u8]) -> R) -> Option<R> {
    let exists = get_u8(data);
    if exists == 0 {
        return None;
    }

    Some(getter(data))
}

#[inline]
fn get_array<'a, const N: usize>(data: &mut &'a [u8]) -> &'a [u8; N] {
    let bytes = &data[..N];
    *data = &data[N..];

    unsafe { &*bytes.as_ptr().cast::<[u8; N]>() }
}

#[inline]
fn get_slice<'a>(data: &mut &'a [u8]) -> &'a [u8] {
    let len = get_len(data);

    let bytes = &data[..len];
    *data = &data[len..];

    bytes
}

#[inline]
fn get_creators<'a>(data: &mut &'a [u8]) -> Option<&'a [Creator]> {
    let getter = |data: &mut &'a [u8]| -> &'a [Creator] {
        let len = get_len(data);
        if len == 0 {
            return &[];
        }

        let bytes_len = len * std::mem::size_of::<Creator>();

        let bytes = &data[..bytes_len];
        *data = &data[bytes_len..];

        let ptr = bytes.as_ptr().cast::<Creator>();
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };

    get_option(data, getter)
}
