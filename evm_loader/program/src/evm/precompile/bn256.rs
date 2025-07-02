use ethnum::U256;
use solana_bn254::prelude::*;

/// Call inner `bn256Add`
#[must_use]
pub fn bn256_add(input: &[u8]) -> Vec<u8> {
    if input.len() >= ALT_BN128_ADDITION_INPUT_LEN {
        alt_bn128_addition(&input[..ALT_BN128_ADDITION_INPUT_LEN])
    } else {
        let mut buffer = vec![0_u8; ALT_BN128_ADDITION_INPUT_LEN];
        buffer[..input.len()].copy_from_slice(input);
        alt_bn128_addition(&buffer)
    }
    .unwrap_or_else(|_| vec![0_u8; ALT_BN128_ADDITION_OUTPUT_LEN])
}

/// Call inner `bn256ScalarMul`
#[must_use]
pub fn bn256_scalar_mul(input: &[u8]) -> Vec<u8> {
    const INPUT_LEN: usize = 96;

    if input.len() >= INPUT_LEN {
        alt_bn128_multiplication(&input[..INPUT_LEN])
    } else {
        let mut buffer = vec![0_u8; INPUT_LEN];
        buffer[..input.len()].copy_from_slice(input);
        alt_bn128_multiplication(&buffer)
    }
    .unwrap_or_else(|_| vec![0_u8; ALT_BN128_MULTIPLICATION_OUTPUT_LEN])
}

/// Call inner `bn256Pairing`
#[must_use]
pub fn bn256_pairing(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return U256::ONE.to_be_bytes().to_vec();
    }

    if (input.len() % ALT_BN128_PAIRING_ELEMENT_LEN) != 0 {
        return U256::ZERO.to_be_bytes().to_vec();
    }

    alt_bn128_pairing(input).unwrap_or_else(|_| vec![0_u8; ALT_BN128_PAIRING_OUTPUT_LEN])
}
