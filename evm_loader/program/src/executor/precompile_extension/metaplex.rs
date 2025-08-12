#![allow(clippy::unnecessary_wraps)]
use std::convert::{Into, TryInto};

use arrayvec::ArrayString;
use ethnum::U256;
use maybe_async::maybe_async;

use solana_program::pubkey::Pubkey;

use crate::{
    account::pda,
    error::{Error, Result},
    executor::external_programs::metaplex::{
        CreateMasterEdition, CreateMetadata, MasterEdition, Metadata, Metaplex,
        MPL_TOKEN_METADATA_ID,
    },
    platform::KeysIndex,
    types::{seeds::Seeds, Address},
};

use super::PrecompileDatabase;

// "[0xc5, 0x73, 0x50, 0xc6]": "createMetadata(bytes32,string,string,string)"
// "[0x4a, 0xe8, 0xb6, 0x6b]": "createMasterEdition(bytes32,uint64)"
// "[0xf7, 0xb6, 0x37, 0xbb]": "isInitialized(bytes32)"
// "[0x23, 0x5b, 0x2b, 0x94]": "isNFT(bytes32)"
// "[0x9e, 0xd1, 0x9d, 0xdb]": "uri(bytes32)"
// "[0x69, 0x1f, 0x34, 0x31]": "name(bytes32)"
// "[0x6b, 0xaa, 0x03, 0x30]": "symbol(bytes32)"

#[maybe_async]
pub async fn metaplex(
    state: &mut impl PrecompileDatabase,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    is_static: bool,
) -> Result<Vec<u8>> {
    if context.value != 0 {
        return Err(Error::Custom("Metaplex: value != 0".to_string()));
    }

    if &context.contract != address {
        return Err(Error::Custom(
            "Metaplex: callcode or delegatecall is not allowed".to_string(),
        ));
    }

    let (selector, input) = input.split_at(4);
    let selector: [u8; 4] = selector.try_into()?;

    match selector {
        [0xc5, 0x73, 0x50, 0xc6] => {
            // "createMetadata(bytes32,string,string,string)"
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let mint = read_pubkey(input)?;
            let name = read_string(input, 32, 256)?;
            let symbol = read_string(input, 64, 256)?;
            let uri = read_string(input, 96, 1024)?;

            create_metadata(context, state, mint, name, symbol, uri).await
        }
        [0x4a, 0xe8, 0xb6, 0x6b] => {
            // "createMasterEdition(bytes32,uint64)"
            if is_static {
                return Err(Error::StaticModeViolation(*address));
            }

            let mint = read_pubkey(input)?;
            let max_supply = read_u64(&input[32..])?;

            create_master_edition(context, state, mint, Some(max_supply)).await
        }
        [0xf7, 0xb6, 0x37, 0xbb] => {
            // "isInitialized(bytes32)"
            let mint = read_pubkey(input)?;
            is_initialized(state, mint).await
        }
        [0x23, 0x5b, 0x2b, 0x94] => {
            // "isNFT(bytes32)"
            let mint = read_pubkey(input)?;
            is_nft(state, mint).await
        }
        [0x9e, 0xd1, 0x9d, 0xdb] => {
            // "uri(bytes32)"
            let mint = read_pubkey(input)?;
            uri(state, mint).await
        }
        [0x69, 0x1f, 0x34, 0x31] => {
            // "name(bytes32)"
            let mint = read_pubkey(input)?;
            token_name(state, mint).await
        }
        [0x6b, 0xaa, 0x03, 0x30] => {
            // "symbol(bytes32)"
            let mint = read_pubkey(input)?;
            symbol(state, mint).await
        }
        _ => Err(Error::UnknownPrecompileMethodSelector(*address, selector)),
    }
}

#[inline]
fn read_u64(input: &[u8]) -> Result<u64> {
    if input.len() < 32 {
        return Err(Error::OutOfBounds);
    }
    U256::from_be_bytes(*arrayref::array_ref![input, 0, 32])
        .try_into()
        .map_err(Into::into)
}

#[inline]
fn read_pubkey(input: &[u8]) -> Result<Pubkey> {
    if input.len() < 32 {
        return Err(Error::OutOfBounds);
    }
    Ok(Pubkey::new_from_array(*arrayref::array_ref![input, 0, 32]))
}

#[inline]
fn read_string(input: &[u8], offset_position: usize, max_length: usize) -> Result<&str> {
    if input.len() < offset_position + 32 {
        return Err(Error::OutOfBounds);
    }
    let offset: usize =
        U256::from_be_bytes(*arrayref::array_ref![input, offset_position, 32]).try_into()?;
    if input.len() < offset.saturating_add(32) {
        return Err(Error::OutOfBounds);
    }
    let length = U256::from_be_bytes(*arrayref::array_ref![input, offset, 32]).try_into()?;
    if length > max_length {
        return Err(Error::OutOfBounds);
    }

    let begin = offset.saturating_add(32);
    let end = begin.saturating_add(length);

    if input.len() < end {
        return Err(Error::OutOfBounds);
    }

    let str = std::str::from_utf8(&input[begin..end])?;
    Ok(str)
}

#[maybe_async]
async fn create_metadata(
    context: &crate::evm::Context,
    state: &mut impl PrecompileDatabase,
    mint: Pubkey,
    name: &str,
    symbol: &str,
    uri: &str,
) -> Result<Vec<u8>> {
    let signer = context.caller;

    let (signer_pubkey, bump_seed) = state.keys().contract_bump(signer);
    let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

    let (metadata_pubkey, _) = Metadata::find_pda(&mint);

    let create_metadata = Metaplex::CreateMetadata(CreateMetadata {
        authority: signer_pubkey,
        seeds: Seeds::new(seeds),
        metadata: metadata_pubkey,
        mint,
        name: ArrayString::from(name).map_err(|_| "Name too long")?,
        symbol: ArrayString::from(symbol).map_err(|_| "Symbol too long")?,
        uri: ArrayString::from(uri).map_err(|_| "URI too long")?,
    });

    state.queue_invoke(create_metadata).await?;

    Ok(metadata_pubkey.to_bytes().to_vec())
}

#[maybe_async]
async fn create_master_edition(
    context: &crate::evm::Context,
    state: &mut impl PrecompileDatabase,
    mint: Pubkey,
    max_supply: Option<u64>,
) -> Result<Vec<u8>> {
    let signer = context.caller;

    let (signer_pubkey, bump_seed) = state.keys().contract_bump(signer);
    let seeds: &[&[u8]] = pda::contract_seeds!(signer, bump_seed);

    let (metadata_pubkey, _) = Metadata::find_pda(&mint);
    let (edition_pubkey, _) = MasterEdition::find_pda(&mint);

    let create_edition = Metaplex::CreateMasterEdition(CreateMasterEdition {
        authority: signer_pubkey,
        seeds: Seeds::new(seeds),
        metadata: metadata_pubkey,
        edition: edition_pubkey,
        mint,
        max_supply,
    });

    state.queue_invoke(create_edition).await?;

    Ok(edition_pubkey.to_bytes().to_vec())
}

#[maybe_async]
async fn is_initialized(state: &impl PrecompileDatabase, mint: Pubkey) -> Result<Vec<u8>> {
    let is_initialized = use_metadata(state, mint, |_| true).await?.unwrap_or(false);

    Ok(to_solidity_bool(is_initialized))
}

#[maybe_async]
async fn is_nft(state: &impl PrecompileDatabase, mint: Pubkey) -> Result<Vec<u8>> {
    let is_nft = use_metadata(state, mint, |m| {
        m.token_standard() == Some(0 /*NonFungible*/)
    })
    .await?
    .unwrap_or(false);

    Ok(to_solidity_bool(is_nft))
}

#[maybe_async]
async fn uri(state: &impl PrecompileDatabase, mint: Pubkey) -> Result<Vec<u8>> {
    let uri = use_metadata(state, mint, |metadata| metadata.uri.to_vec())
        .await?
        .unwrap_or_else(Vec::new);

    let uri = String::from_utf8(uri)?;
    Ok(to_solidity_string(uri.trim_end_matches('\0')))
}

#[maybe_async]
async fn token_name(state: &impl PrecompileDatabase, mint: Pubkey) -> Result<Vec<u8>> {
    let name = use_metadata(state, mint, |metadata| metadata.name.to_vec())
        .await?
        .unwrap_or_else(Vec::new);

    let name = String::from_utf8(name)?;
    Ok(to_solidity_string(name.trim_end_matches('\0')))
}

#[maybe_async]
async fn symbol(state: &impl PrecompileDatabase, mint: Pubkey) -> Result<Vec<u8>> {
    let symbol = use_metadata(state, mint, |metadata| metadata.symbol.to_vec())
        .await?
        .unwrap_or_else(Vec::new);

    let symbol = String::from_utf8(symbol)?;
    Ok(to_solidity_string(symbol.trim_end_matches('\0')))
}

#[maybe_async]
async fn use_metadata<R>(
    state: &impl PrecompileDatabase,
    mint: Pubkey,
    f: impl FnOnce(&Metadata) -> R,
) -> Result<Option<R>> {
    let (metadata_pubkey, _) = Metadata::find_pda(&mint);
    let metadata_account = state.external_account(&metadata_pubkey).await?;

    if metadata_account.owner != MPL_TOKEN_METADATA_ID {
        return Ok(None);
    }

    let metadata = Metadata::deserialize(&metadata_account.data);
    let result = f(&metadata);

    Ok(Some(result))
}

fn to_solidity_bool(v: bool) -> Vec<u8> {
    let mut result = vec![0_u8; 32];
    result[31] = u8::from(v);
    result
}

fn to_solidity_string(s: &str) -> Vec<u8> {
    // String encoding
    // 32 bytes - offset
    // 32 bytes - length
    // length + padding bytes - data

    let data_len = if s.len() % 32 == 0 {
        std::cmp::max(s.len(), 32)
    } else {
        ((s.len() / 32) + 1) * 32
    };

    let mut result = vec![0_u8; 32 + 32 + data_len];

    result[31] = 0x20; // offset - 32 bytes

    let length = U256::new(s.len() as u128);
    result[32..64].copy_from_slice(&length.to_be_bytes());

    result[64..64 + s.len()].copy_from_slice(s.as_bytes());

    result
}
