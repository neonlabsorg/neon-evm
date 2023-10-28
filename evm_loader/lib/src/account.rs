use evm_loader::account::Packable;
use evm_loader::error;
use evm_loader::error::Error;
use evm_loader::solana_program::pubkey::Pubkey;
use solana_sdk::account::Account;

pub fn from_account<T: Packable>(
    program_id: Pubkey,
    key: Pubkey,
    info: &Account,
) -> error::Result<T> {
    if info.owner != program_id {
        return Err(Error::AccountInvalidOwner(key, program_id));
    }

    let parts = split_account_data(key, &info.data[..], T::SIZE)?;
    if *parts.tag != T::TAG {
        return Err(Error::AccountInvalidTag(key, T::TAG));
    }

    Ok(T::unpack(parts.data))
}

fn split_account_data(
    key: Pubkey,
    account_data: &[u8],
    data_len: usize,
) -> error::Result<AccountParts> {
    if account_data.len() < 1 + data_len {
        return Err(Error::AccountInvalidData(key));
    }

    let (tag, bytes) = account_data.split_first().expect("data is not empty");
    let (data, remaining) = bytes.split_at(data_len);

    Ok(AccountParts {
        tag,
        data,
        _remaining: remaining,
    })
}

struct AccountParts<'a> {
    tag: &'a u8,
    data: &'a [u8],
    _remaining: &'a [u8],
}
