use std::mem::size_of;

use solana_program::pubkey::Pubkey;

#[cfg(target_os = "solana")]
mod solana_allocator;

// Holder account heap constants.

/// See [`solana_program::entrypoint::deserialize`] for more details.
const FIRST_ACCOUNT_DATA_OFFSET: usize =
    /* number of accounts */
    size_of::<u64>() +
    /* duplication marker */ size_of::<u8>() +
    /* is signer? */ size_of::<u8>() +
    /* is writable? */ size_of::<u8>() +
    /* is executable? */ size_of::<u8>() +
    /* original_data_len */ size_of::<u32>() +
    /* key */ size_of::<Pubkey>() +
    /* owner */ size_of::<Pubkey>() +
    /* lamports */ size_of::<u64>() +
    /* factual_data_len */ size_of::<u64>();

/// See <https://solana.com/docs/programs/faq#memory-map> for more details.
const PROGRAM_DATA_INPUT_PARAMETERS_OFFSET: usize = 0x0004_0000_0000_usize;

pub const STATE_ACCOUNT_DATA_ADDRESS: usize =
    PROGRAM_DATA_INPUT_PARAMETERS_OFFSET + FIRST_ACCOUNT_DATA_OFFSET;

#[cfg(target_os = "solana")]
pub type StateAllocator = solana_allocator::SolanaAllocator;

#[cfg(target_os = "solana")]
#[must_use]
pub fn acc_allocator() -> StateAllocator {
    solana_allocator::SolanaAllocator::static_account_alloc()
}

#[cfg(not(target_os = "solana"))]
pub type StateAllocator = std::alloc::System;

#[cfg(not(target_os = "solana"))]
#[must_use]
pub fn acc_allocator() -> StateAllocator {
    std::alloc::System {}
}

pub type StateAccountAllocator = StateAllocator;
