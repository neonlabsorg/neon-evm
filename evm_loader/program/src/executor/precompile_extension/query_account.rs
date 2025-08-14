use std::convert::TryInto;

use arrayref::{array_ref, array_refs};
use ethnum::U256;
use maybe_async::maybe_async;
use solana_program::pubkey::Pubkey;

use crate::{
    account::AccountRead,
    error::{Error, Result},
    types::Address,
};

use super::PrecompileDatabase;

// QueryAccount method DEPRECATED ids:
//-------------------------------------------
// cache(uint256,uint64,uint64) => 0x2b3c8322
// owner(uint256)               => 0xa123c33e
// length(uint256)              => 0xaa8b99d2
// lamports(uint256)            => 0x748f2d8a
// executable(uint256)          => 0xc219a785
// rent_epoch(uint256)          => 0xc4d369b5
// data(uint256,uint64,uint64)  => 0x43ca5161
//-------------------------------------------
// QueryAccount method current ids:
// "02571be3": "owner(bytes32)",
// "6273448f": "lamports(bytes32)",
// "e6bef488": "executable(bytes32)",
// "8bb9e6f4": "rent_epoch(bytes32)"
// "b64a097e": "info(bytes32)",
// "a9dbaf25": "length(bytes32)",
// "7dd6c1a0": "data(bytes32,uint64,uint64)",

#[maybe_async]
pub async fn query_account(
    state: &impl PrecompileDatabase,
    address: &Address,
    input: &[u8],
    context: &crate::evm::Context,
    _is_static: bool,
) -> Result<Vec<u8>> {
    debug_print!("query_account({})", hex::encode(input));

    if context.value != 0 {
        return Err(Error::Custom("Query Account: value != 0".to_string()));
    }

    let (method_id, rest) = input.split_at(4);
    let method_id: [u8; 4] = method_id.try_into()?;

    let (account_address, rest) = rest.split_at(32);
    let pubkey = Pubkey::try_from(account_address)?;

    match method_id {
        [0x2b, 0x3c, 0x83, 0x22] => {
            // cache(uint256,uint64,uint64)
            // deprecated
            Ok(vec![])
        }
        [0xa1, 0x23, 0xc3, 0x3e] | [0x02, 0x57, 0x1b, 0xe3] => {
            debug_print!("query_account.owner({})", &pubkey);
            account_owner(state, &pubkey).await
        }
        [0xaa, 0x8b, 0x99, 0xd2] | [0xa9, 0xdb, 0xaf, 0x25] => {
            debug_print!("query_account.length({})", &pubkey);
            account_data_length(state, &pubkey).await
        }
        [0x74, 0x8f, 0x2d, 0x8a] | [0x62, 0x73, 0x44, 0x8f] => {
            debug_print!("query_account.lamports({})", &pubkey);
            account_lamports(state, &pubkey).await
        }
        [0xc2, 0x19, 0xa7, 0x85] | [0xe6, 0xbe, 0xf4, 0x88] => {
            debug_print!("query_account.executable({})", &pubkey);
            account_is_executable(state, &pubkey).await
        }
        [0xc4, 0xd3, 0x69, 0xb5] | [0x8b, 0xb9, 0xe6, 0xf4] => {
            debug_print!("query_account.rent_epoch({})", &pubkey);
            account_rent_epoch(state, &pubkey).await
        }
        [0x43, 0xca, 0x51, 0x61] | [0x7d, 0xd6, 0xc1, 0xa0] => {
            let arguments = array_ref![rest, 0, 64];
            let (offset, length) = array_refs!(arguments, 32, 32);
            let offset = U256::from_be_bytes(*offset).try_into()?;
            let length = U256::from_be_bytes(*length).try_into()?;
            debug_print!("query_account.data({}, {}, {})", pubkey, offset, length);
            account_data(state, &pubkey, offset, length).await
        }
        [0xb6, 0x4a, 0x09, 0x7e] => {
            debug_print!("query_account.info({})", &pubkey);
            account_info(state, &pubkey).await
        }
        _ => {
            debug_print!("query_account UNKNOWN {:?}", method_id);
            Err(Error::UnknownPrecompileMethodSelector(*address, method_id))
        }
    }
}

#[maybe_async]
async fn account_owner(state: &impl PrecompileDatabase, address: &Pubkey) -> Result<Vec<u8>> {
    let owner = state.use_raw_account(address, |a| a.owner()).await?;

    let bytes = owner.to_bytes().to_vec();
    Ok(bytes)
}

#[maybe_async]
async fn account_lamports(state: &impl PrecompileDatabase, address: &Pubkey) -> Result<Vec<u8>> {
    let lamports: U256 = state
        .use_raw_account(address, |a| a.lamports().into())
        .await?;

    let bytes = lamports.to_be_bytes().to_vec();
    Ok(bytes)
}

#[maybe_async]
async fn account_rent_epoch(state: &impl PrecompileDatabase, address: &Pubkey) -> Result<Vec<u8>> {
    let epoch: U256 = state
        .use_raw_account(address, |a| a.rent_epoch().into())
        .await?;

    let bytes = epoch.to_be_bytes().to_vec();
    Ok(bytes)
}

#[maybe_async]
async fn account_is_executable(
    state: &impl PrecompileDatabase,
    address: &Pubkey,
) -> Result<Vec<u8>> {
    let executable: U256 = state
        .use_raw_account(address, |a| a.is_executable().into())
        .await?;

    let bytes = executable.to_be_bytes().to_vec();
    Ok(bytes)
}

#[maybe_async]
async fn account_data_length(state: &impl PrecompileDatabase, address: &Pubkey) -> Result<Vec<u8>> {
    let data_len: usize = state.use_raw_account(address, |a| a.data_len()).await?;

    let data_len: U256 = data_len.try_into()?;
    let bytes = data_len.to_be_bytes().to_vec();
    Ok(bytes)
}

#[maybe_async]
async fn account_data(
    state: &impl PrecompileDatabase,
    address: &Pubkey,
    offset: usize,
    length: usize,
) -> Result<Vec<u8>> {
    if length == 0 {
        return Err(Error::Custom(
            "Query Account: data() - length == 0".to_string(),
        ));
    }

    state
        .use_raw_account(address, |a| {
            a.data().get(offset..offset + length).map(<[u8]>::to_vec)
        })
        .await?
        .ok_or_else(|| Error::Custom("Query Account: data() - out of bounds".to_string()))
}

#[maybe_async]
async fn account_info(state: &impl PrecompileDatabase, address: &Pubkey) -> Result<Vec<u8>> {
    fn to_solidity_account_value(info: &impl AccountRead) -> Vec<u8> {
        let mut buffer = [0_u8; 5 * 32];
        let (key, _, lamports, owner, _, executable, _, rent_epoch) =
            arrayref::mut_array_refs![&mut buffer, 32, 24, 8, 32, 31, 1, 24, 8];

        *key = info.pubkey().to_bytes();
        *lamports = info.lamports().to_be_bytes();
        *owner = info.owner().to_bytes();
        executable[0] = info.is_executable().into();
        *rent_epoch = info.rent_epoch().to_be_bytes();

        buffer.to_vec()
    }

    let info = state
        .use_raw_account(address, |a| to_solidity_account_value(&a))
        .await?;
    Ok(info)
}
