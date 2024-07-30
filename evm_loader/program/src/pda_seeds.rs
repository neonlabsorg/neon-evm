use crate::account::Operator;
use crate::types::Address;
use ethnum::U256;
use solana_program::pubkey::Pubkey;

pub const AUTHORITY_SEEDS: &[&[u8]] = &[b"Deposit"];

const ACCOUNT_SEED_VERSION_SLICE: &[u8] = &[crate::config::ACCOUNT_SEED_VERSION];

#[must_use]
pub fn with_balance_account_seeds<R>(
    address: &Address,
    chain_id: u64,
    bump_seed: &[u8],
    f: impl Fn(&[&[u8]]) -> R,
) -> R {
    f(&[
        ACCOUNT_SEED_VERSION_SLICE,
        address.as_bytes(),
        &U256::from(chain_id).to_be_bytes(),
        bump_seed,
    ])
}

#[must_use]
pub fn contract_account_seeds<'a>(address: &'a Address, bump_seed: &'a [u8]) -> [&'a [u8]; 3] {
    [ACCOUNT_SEED_VERSION_SLICE, address.as_bytes(), bump_seed]
}

fn to_vec_vec(seeds: &[&[u8]]) -> Vec<Vec<u8>> {
    seeds.iter().map(|v| v.to_vec()).collect()
}

#[must_use]
pub fn contract_account_seeds_vec(address: &Address, bump_seed: u8) -> Vec<Vec<u8>> {
    to_vec_vec(&contract_account_seeds(address, &[bump_seed]))
}

#[must_use]
pub fn spl_token_seeds<'a>(address: &'a Address, seed: &'a [u8]) -> [&'a [u8]; 4] {
    [
        ACCOUNT_SEED_VERSION_SLICE,
        b"ContractData",
        address.as_bytes(),
        seed,
    ]
}

#[must_use]
pub fn external_authority_seeds<'a>(address: &'a Address, seed: &'a [u8]) -> [&'a [u8]; 4] {
    [
        ACCOUNT_SEED_VERSION_SLICE,
        b"AUTH",
        address.as_bytes(),
        seed,
    ]
}

#[must_use]
pub fn with_treasury_seeds<R>(index: u32, bump_seed: &[u8], f: impl Fn(&[&[u8]]) -> R) -> R {
    f(&[
        crate::config::TREASURY_POOL_SEED.as_bytes(),
        &index.to_le_bytes(),
        bump_seed,
    ])
}

#[must_use]
pub fn main_treasury_seeds(bump_seed: &[u8]) -> [&[u8]; 2] {
    [crate::config::TREASURY_POOL_SEED.as_bytes(), bump_seed]
}

#[must_use]
pub fn with_operator_seeds<R>(
    operator: &Operator<'_>,
    address: &Address,
    chain_id: u64,
    bump_seed: &[u8],
    f: impl Fn(&[&[u8]]) -> R,
) -> R {
    f(&[
        ACCOUNT_SEED_VERSION_SLICE,
        operator.key.as_ref(),
        address.as_bytes(),
        &U256::from(chain_id).to_be_bytes(),
        bump_seed,
    ])
}

#[must_use]
pub fn payer_seeds<'a>(address: &'a Address, bump_seed: &'a [u8]) -> [&'a [u8]; 4] {
    [
        ACCOUNT_SEED_VERSION_SLICE,
        b"PAYER",
        address.as_bytes(),
        bump_seed,
    ]
}

#[must_use]
pub fn payer_seeds_vec(address: &Address, bump_seed: u8) -> Vec<Vec<u8>> {
    to_vec_vec(&payer_seeds(address, &[bump_seed]))
}

fn iter_map_collect<T>(seeds: &[Vec<T>]) -> Vec<&[T]> {
    seeds.iter().map(Vec::as_slice).collect::<Vec<_>>()
}

pub fn with_slice_of_slice_of_slice<R>(seeds: &[Vec<Vec<u8>>], f: impl Fn(&[&[&[u8]]]) -> R) -> R {
    f(&iter_map_collect(
        &seeds
            .iter()
            .map(|s| iter_map_collect(s))
            .collect::<Vec<_>>(),
    ))
}

pub trait PubkeyExt {
    fn find_program_address_with_seeds(
        seeds: &[&[u8]],
        program_id: &Pubkey,
    ) -> (Pubkey, Vec<Vec<u8>>);
}

impl PubkeyExt for Pubkey {
    fn find_program_address_with_seeds(
        seeds: &[&[u8]],
        program_id: &Pubkey,
    ) -> (Pubkey, Vec<Vec<u8>>) {
        let (pubkey, bump_seed) = Pubkey::find_program_address(seeds, program_id);

        let mut seeds: Vec<_> = seeds.iter().map(|v| v.to_vec()).collect();
        seeds.push(vec![bump_seed]);

        (pubkey, seeds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Address;
    use hex::FromHex;
    use std::str::FromStr;

    #[test]
    fn test_authority_pubkey_mainnet() {
        let neon_evm = Pubkey::from_str("NeonVMyRX5GbCrsAHnUwx1nYYoJAtskU1bWUo6JGNyG").unwrap();

        let (pubkey, _) = Pubkey::find_program_address(AUTHORITY_SEEDS, &neon_evm);

        assert_eq!(
            pubkey.to_string(),
            "CUU8HLwbSc2zFEDenmauiEJbCGCNy4eAHAmznZcjB6Nn"
        );
    }

    #[test]
    fn test_usdt_pubkey_mainnet() {
        let neon_evm = Pubkey::from_str("NeonVMyRX5GbCrsAHnUwx1nYYoJAtskU1bWUo6JGNyG").unwrap();

        // Neon USDT token: https://neonscan.org/token/0x5f0155d08ef4aae2b500aefb64a3419da8bb611a
        let usdt_address = Address::from_hex("0x5f0155d08eF4aaE2B500AefB64A3419dA8bB611a").unwrap();

        let (usdt_pubkey, _) =
            Pubkey::find_program_address(&contract_account_seeds(&usdt_address, &[]), &neon_evm);

        assert_eq!(
            usdt_pubkey.to_string(),
            "GHuABgXXF37MqV9WyqJXwvzA2eLkcxKf2t8WbiVzBLnU"
        );
    }

    // Neon tx: https://neonscan.org/tx/0x0729687b2f56398652a6593b87b9932f3fe2f2e0c778eb4841a4e17d961a2a11
    #[test]
    fn test_token_account_pubkey_mainnet() {
        let neon_evm = Pubkey::from_str("NeonVMyRX5GbCrsAHnUwx1nYYoJAtskU1bWUo6JGNyG").unwrap();

        // Neon USDT token: https://neonscan.org/token/0x5f0155d08ef4aae2b500aefb64a3419da8bb611a
        let usdt_address = Address::from_hex("0x5f0155d08eF4aaE2B500AefB64A3419dA8bB611a").unwrap();

        // Neon tx.from address: https://neonscan.org/address/0x35b6c40e3873f361c43c073154bf8b37c1f34cd7
        let address = <[u8; 32]>::from_hex(
            "00000000000000000000000035B6C40e3873F361c43c073154BF8b37C1f34Cd7",
        )
        .unwrap();

        let (pubkey, _) =
            Pubkey::find_program_address(&spl_token_seeds(&usdt_address, &address), &neon_evm);

        assert_eq!(
            pubkey.to_string(),
            "12HWB2U31J5AMgDTaaXBdNGN8jAeJNiwpgkewCNVNKyU"
        );
    }
}
