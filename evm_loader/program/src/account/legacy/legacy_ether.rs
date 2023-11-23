use std::mem::size_of;

use ethnum::U256;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::{config::STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT, error::Result, types::Address};

pub struct LegacyEtherData {
    /// Ethereum address
    pub address: Address,
    /// Solana account nonce
    pub bump_seed: u8,
    /// Ethereum account nonce
    pub trx_count: u64,
    /// Neon token balance
    pub balance: U256,
    /// Account generation, increment on suicide
    pub generation: u32,
    /// Contract code size
    pub code_size: u32,
    /// Read-write lock
    pub rw_blocked: bool,
}

impl LegacyEtherData {
    const ADDRESS_SIZE: usize = size_of::<Address>();
    const BUMP_SEED_SIZE: usize = size_of::<u8>();
    const TRX_COUNT_SIZE: usize = size_of::<u64>();
    const BALANCE_SIZE: usize = size_of::<U256>();
    const GENERATION_SIZE: usize = size_of::<u32>();
    const CODE_SIZE_SIZE: usize = size_of::<u32>();
    const RW_BLOCKED_SIZE: usize = size_of::<bool>();

    /// `AccountV3` struct tag
    pub const TAG: u8 = super::TAG_ACCOUNT_CONTRACT_DEPRECATED;

    /// `AccountV3` struct serialized size
    pub const SIZE: usize = Self::ADDRESS_SIZE
        + Self::BUMP_SEED_SIZE
        + Self::TRX_COUNT_SIZE
        + Self::BALANCE_SIZE
        + Self::GENERATION_SIZE
        + Self::CODE_SIZE_SIZE
        + Self::RW_BLOCKED_SIZE;

    #[must_use]
    pub fn unpack(input: &[u8]) -> Self {
        let data = arrayref::array_ref![input, 0, LegacyEtherData::SIZE];
        #[allow(clippy::ptr_offset_with_cast)]
        let (address, bump_seed, trx_count, balance, generation, code_size, rw_blocked) = arrayref::array_refs![
            data,
            LegacyEtherData::ADDRESS_SIZE,
            LegacyEtherData::BUMP_SEED_SIZE,
            LegacyEtherData::TRX_COUNT_SIZE,
            LegacyEtherData::BALANCE_SIZE,
            LegacyEtherData::GENERATION_SIZE,
            LegacyEtherData::CODE_SIZE_SIZE,
            LegacyEtherData::RW_BLOCKED_SIZE
        ];

        Self {
            address: Address::from(*address),
            bump_seed: bump_seed[0],
            trx_count: u64::from_le_bytes(*trx_count),
            balance: U256::from_le_bytes(*balance),
            generation: u32::from_le_bytes(*generation),
            code_size: u32::from_le_bytes(*code_size),
            rw_blocked: rw_blocked[0] != 0,
        }
    }

    pub fn from_account(program_id: &Pubkey, account: &AccountInfo) -> Result<Self> {
        crate::account::validate_tag(program_id, account, Self::TAG)?;

        let data = account.try_borrow_data()?;
        Ok(Self::unpack(&data[1..]))
    }

    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn read_storage(&self, account: &AccountInfo) -> Vec<[u8; 32]> {
        if self.code_size == 0 {
            return vec![[0; 32]; STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT];
        }

        let data = account.data.borrow();

        let storage_offset = 1 + Self::SIZE;
        let storage_len = 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;

        let storage = &data[storage_offset..][..storage_len];
        let storage = unsafe {
            // storage_len is multiple of STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT
            // [u8; 32] has the same alignment as u8
            let ptr: *const [u8; 32] = storage.as_ptr().cast();
            std::slice::from_raw_parts(ptr, STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT)
        };

        storage.to_vec()
    }

    #[must_use]
    pub fn read_code(&self, account: &AccountInfo) -> Vec<u8> {
        if self.code_size == 0 {
            return Vec::new();
        }

        let data = account.data.borrow();

        let storage_offset = 1 + Self::SIZE;
        let storage_len = 32 * STORAGE_ENTRIES_IN_CONTRACT_ACCOUNT;

        let code_offset = storage_offset + storage_len;
        let code_len = self.code_size as usize;

        let code = &data[code_offset..][..code_len];
        code.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::str::FromStr;

    // Neon Tx hash: 0x5875c2f6395560ee537297eb29e81e222ebbaffb5cace60bb13a48b931845ef0
    #[test]
    fn test_deserialize_legacy_ether_data_from_account_before_tx() {
        let mut lamports = 0;
        let mut data = base64::decode("DIIhGTTDQLKVYTgTkjSNSEE+Fa3I/hQAAAAAAAAAYDWpG9a+FocEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        let owner = Pubkey::new_unique();

        let account_info = AccountInfo {
            key: &Pubkey::new_unique(),
            lamports: Rc::new(RefCell::new(&mut lamports)),
            data: Rc::new(RefCell::new(&mut data)),
            owner: &owner,
            rent_epoch: 0,
            is_signer: false,
            is_writable: false,
            executable: false,
        };

        let legacy_ether_data = LegacyEtherData::from_account(&owner, &account_info).unwrap();
        assert_eq!(
            legacy_ether_data.address,
            Address::from_str("0x82211934c340b29561381392348d48413e15adc8").unwrap()
        );
        assert_eq!(legacy_ether_data.bump_seed, 254);
        assert_eq!(legacy_ether_data.trx_count, 20);
        assert_eq!(legacy_ether_data.balance, 83_521_153_766_242_465_120);
        assert_eq!(legacy_ether_data.generation, 0);
        assert_eq!(legacy_ether_data.code_size, 0);
        assert!(!legacy_ether_data.rw_blocked);

        assert_eq!(
            legacy_ether_data
                .address
                .find_solana_address(
                    &Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU").unwrap()
                )
                .0,
            Pubkey::from_str("4MCcX687JtEUjp3gQncDQxVWyQEfTtY6PP6jBXH8Z3JM").unwrap()
        );
    }

    // Neon Tx hash: 0x5875c2f6395560ee537297eb29e81e222ebbaffb5cace60bb13a48b931845ef0
    #[test]
    fn test_deserialize_legacy_ether_data_from_account_after_tx() {
        let mut lamports = 0;
        let mut data = base64::decode("DIIhGTTDQLKVYTgTkjSNSEE+Fa3I/hUAAAAAAAAAkBWIGQpybVwEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        let owner = Pubkey::new_unique();

        let account_info = AccountInfo {
            key: &Pubkey::new_unique(),
            lamports: Rc::new(RefCell::new(&mut lamports)),
            data: Rc::new(RefCell::new(&mut data)),
            owner: &owner,
            rent_epoch: 0,
            is_signer: false,
            is_writable: false,
            executable: false,
        };

        let legacy_ether_data = LegacyEtherData::from_account(&owner, &account_info).unwrap();
        assert_eq!(
            legacy_ether_data.address,
            Address::from_str("0x82211934c340b29561381392348d48413e15adc8").unwrap()
        );
        assert_eq!(legacy_ether_data.bump_seed, 254);
        assert_eq!(legacy_ether_data.trx_count, 21);
        assert_eq!(legacy_ether_data.balance, 80_447_081_106_492_626_320);
        assert_eq!(legacy_ether_data.generation, 0);
        assert_eq!(legacy_ether_data.code_size, 0);
        assert!(!legacy_ether_data.rw_blocked);

        assert_eq!(
            legacy_ether_data
                .address
                .find_solana_address(
                    &Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU").unwrap()
                )
                .0,
            Pubkey::from_str("4MCcX687JtEUjp3gQncDQxVWyQEfTtY6PP6jBXH8Z3JM").unwrap()
        );

        assert_eq!(
            U256::from_str("83521153766242465120").unwrap()
                - U256::from_str("80447081106492626320").unwrap(),
            U256::from_str("3074072659749838800").unwrap()
        );
    }

    #[test]
    fn test_deserialize_legacy_ether_data_operator_account_after_tx() {
        let mut lamports = 0;
        let mut data = base64::decode("DLj96bgwrCSg5dGWwC6QSXedngjV/gAAAAAAAAAAGLA73pkXZTsUAwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        let owner = Pubkey::new_unique();

        let account_info = AccountInfo {
            key: &Pubkey::new_unique(),
            lamports: Rc::new(RefCell::new(&mut lamports)),
            data: Rc::new(RefCell::new(&mut data)),
            owner: &owner,
            rent_epoch: 0,
            is_signer: false,
            is_writable: false,
            executable: false,
        };

        let legacy_ether_data = LegacyEtherData::from_account(&owner, &account_info).unwrap();
        assert_eq!(
            legacy_ether_data.address,
            Address::from_str("0xb8fde9b830ac24a0e5d196c02e9049779d9e08d5").unwrap()
        );
        assert_eq!(legacy_ether_data.bump_seed, 254);
        assert_eq!(legacy_ether_data.trx_count, 0);
        assert_eq!(legacy_ether_data.balance, 14_540_314_183_053_638_086_680);
        assert_eq!(legacy_ether_data.generation, 0);
        assert_eq!(legacy_ether_data.code_size, 0);
        assert!(!legacy_ether_data.rw_blocked);

        assert_eq!(
            legacy_ether_data
                .address
                .find_solana_address(
                    &Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU").unwrap()
                )
                .0,
            Pubkey::from_str("3ei1nFgS2aeEFRJHE9YkydSKgdkyisgbK6gpr5KJc5Qb").unwrap()
        );
    }

    // Neon Tx hash: 0xa191c3fccc1418557937a39a76d5e9c1f2e94b2633a40147302607f6d66ed501
    #[test]
    fn test_deserialize_legacy_ether_data_operator_account_before_tx() {
        let mut lamports = 0;
        let x: [i32; 71] = [
            12, -72, -3, -23, -72, 48, -84, 36, -96, -27, -47, -106, -64, 46, -112, 73, 119, -99,
            -98, 8, -43, -2, 0, 0, 0, 0, 0, 0, 0, 0, 56, -49, 57, -93, -93, -6, -16, 16, 20, 3, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0,
        ];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut data = x.into_iter().map(|v| (v + 256) as u8).collect::<Vec<u8>>();
        let owner = Pubkey::new_unique();

        let account_info = AccountInfo {
            key: &Pubkey::new_unique(),
            lamports: Rc::new(RefCell::new(&mut lamports)),
            data: Rc::new(RefCell::new(&mut data)),
            owner: &owner,
            rent_epoch: 0,
            is_signer: false,
            is_writable: false,
            executable: false,
        };

        let legacy_ether_data = LegacyEtherData::from_account(&owner, &account_info).unwrap();
        assert_eq!(
            legacy_ether_data.address,
            Address::from_str("0xb8fde9b830ac24a0e5d196c02e9049779d9e08d5").unwrap()
        );
        assert_eq!(legacy_ether_data.bump_seed, 254);
        assert_eq!(legacy_ether_data.trx_count, 0);
        assert_eq!(legacy_ether_data.balance, 14_537_255_081_162_869_165_880);
        assert_eq!(legacy_ether_data.generation, 0);
        assert_eq!(legacy_ether_data.code_size, 0);
        assert!(!legacy_ether_data.rw_blocked);

        assert_eq!(
            legacy_ether_data
                .address
                .find_solana_address(
                    &Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU").unwrap()
                )
                .0,
            Pubkey::from_str("3ei1nFgS2aeEFRJHE9YkydSKgdkyisgbK6gpr5KJc5Qb").unwrap()
        );

        assert_eq!(
            U256::from_str("14540314183053638086680").unwrap()
                - U256::from_str("14537255081162869165880").unwrap(),
            U256::from_str("3059101890768920800").unwrap()
        );
    }

    #[test]
    fn test_deserialize_legacy_ether_data_operator_account2() {
        let mut lamports = 0;
        let mut data = base64::decode("DNRaLxGm6ggK1dsEMh+eJmB5+s8U/wAAAAAAAAAAkAM0bBhiorsEAwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap();
        let owner = Pubkey::new_unique();

        let account_info = AccountInfo {
            key: &Pubkey::new_unique(),
            lamports: Rc::new(RefCell::new(&mut lamports)),
            data: Rc::new(RefCell::new(&mut data)),
            owner: &owner,
            rent_epoch: 0,
            is_signer: false,
            is_writable: false,
            executable: false,
        };

        let legacy_ether_data = LegacyEtherData::from_account(&owner, &account_info).unwrap();
        assert_eq!(
            legacy_ether_data.address,
            Address::from_str("0xd45a2f11a6ea080ad5db04321f9e266079facf14").unwrap()
        );
        assert_eq!(legacy_ether_data.bump_seed, 255);
        assert_eq!(legacy_ether_data.trx_count, 0);
        assert_eq!(legacy_ether_data.balance, 14_254_406_901_792_127_583_120);
        assert_eq!(legacy_ether_data.generation, 0);
        assert_eq!(legacy_ether_data.code_size, 0);
        assert!(!legacy_ether_data.rw_blocked);

        assert_eq!(
            legacy_ether_data
                .address
                .find_solana_address(
                    &Pubkey::from_str("eeLSJgWzzxrqKv1UxtRVVH8FX3qCQWUs9QuAjJpETGU").unwrap()
                )
                .0,
            Pubkey::from_str("9LfrbwpW44A6LkNSPWjkXiEcYHoJ3Nkk3o52M9JcWE9G").unwrap()
        );
    }

    #[test]
    fn test_deserialize_pubkey() {
        assert_eq!(
            Pubkey::from_str("3ei1nFgS2aeEFRJHE9YkydSKgdkyisgbK6gpr5KJc5Qb")
                .unwrap()
                .to_bytes(),
            [
                39, 96, 60, 227, 133, 88, 143, 17, 237, 98, 20, 8, 36, 208, 245, 204, 107, 200, 44,
                11, 37, 248, 129, 26, 124, 186, 187, 152, 127, 120, 149, 48
            ]
        );
    }
}
