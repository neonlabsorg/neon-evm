use std::cell::{Ref, RefMut};
use std::cmp::Ordering;
use std::mem::size_of;

use super::{
    Account, AccountDispatch, AccountHeader, NoHeader, ZeroInit, ACCOUNT_PREFIX_LEN, TAG_EMPTY,
    TAG_STORAGE_CELL,
};
use crate::error::{Error, Result};
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

#[derive(Copy, Clone)]
pub struct StorageCellSeed {
    seed: [u8; 32],
}

impl StorageCellSeed {
    #[must_use]
    fn make_seed(index: U256) -> [u8; 32] {
        let mut buffer = [0_u8; 32];

        let index_bytes = index.to_be_bytes();
        let index_bytes = &index_bytes[3..31];

        for i in 0..28 {
            buffer[i] = index_bytes[i] & 0x7F;
        }

        #[allow(clippy::needless_range_loop)]
        for i in 0..7 {
            buffer[28] |= (index_bytes[i] & 0x80) >> (1 + i);
        }
        for i in 0..7 {
            buffer[29] |= (index_bytes[7 + i] & 0x80) >> (1 + i);
        }
        for i in 0..7 {
            buffer[30] |= (index_bytes[14 + i] & 0x80) >> (1 + i);
        }
        for i in 0..7 {
            buffer[31] |= (index_bytes[21 + i] & 0x80) >> (1 + i);
        }

        buffer
    }

    #[must_use]
    pub fn new(index: U256) -> Self {
        let seed = Self::make_seed(index);
        Self { seed }
    }
}

impl std::ops::Deref for StorageCellSeed {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe { std::str::from_utf8_unchecked(&self.seed) }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct Cell {
    pub subindex: u8,
    pub value: [u8; 32],
}

pub struct StorageCell<'a> {
    pub account: Account<'a>, // TODO: make it private after emulator changes
}

#[repr(C, packed)]
pub struct HeaderWithRevision {
    revision: u32,
}
impl AccountHeader for HeaderWithRevision {
    const VERSION: u8 = 2;
}

// Set the last version of the Header struct here
// and change the `header_size` and `header_upgrade` functions
pub type Header = HeaderWithRevision;

impl<'a> StorageCell<'a> {
    #[must_use]
    pub fn required_account_size(cells: usize) -> usize {
        ACCOUNT_PREFIX_LEN + size_of::<Header>() + cells * size_of::<Cell>()
    }

    #[must_use]
    pub fn required_header_realloc(&self) -> usize {
        let allocated_header_size = self.header_size();
        size_of::<Header>().saturating_sub(allocated_header_size)
    }

    pub fn from_account_info(program_id: Pubkey, account: &AccountInfo<'a>) -> Result<Self> {
        let account = account.clone().into();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, account: Account<'a>) -> Result<Self> {
        account.validate_tag(program_id, TAG_STORAGE_CELL)?;

        Ok(Self { account })
    }

    /// # Safety
    /// It's a caller responsibility to validate the account tag
    #[must_use]
    pub unsafe fn from_account_unchecked(account: Account<'a>) -> Self {
        Self { account }
    }

    pub fn initialize(mut account: Account<'a>, program_id: Pubkey) -> Result<Self> {
        assert_eq!(account.data_len(), Self::required_account_size(0));
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_STORAGE_CELL, Header::VERSION)?;
        {
            let mut header: RefMut<Header> = account.header_mut();
            header.revision = 0; // Empty account without cells, no need to increment revision
        }

        Ok(Self { account })
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    fn header_size(&self) -> usize {
        match self.account.header_version() {
            0 | 1 => size_of::<NoHeader>(),
            HeaderWithRevision::VERSION => size_of::<HeaderWithRevision>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    fn header_upgrade(&mut self) -> Result<()> {
        match self.account.header_version() {
            0 | 1 => {
                self.account.expand_header::<NoHeader, Header>()?;
            }
            HeaderWithRevision::VERSION => {
                self.account.expand_header::<HeaderWithRevision, Header>()?;
            }
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }

        Ok(())
    }

    fn cells_offset(&self) -> usize {
        ACCOUNT_PREFIX_LEN + self.header_size()
    }

    #[must_use]
    pub fn cells(&self) -> Ref<[Cell]> {
        let cells_offset = self.cells_offset();

        let data = self.account.data();
        let data = Ref::map(data, |d| &d[cells_offset..]);

        Ref::map(data, |bytes| {
            static_assertions::assert_eq_align!(Cell, u8);
            assert_eq!(bytes.len() % size_of::<Cell>(), 0);

            // SAFETY: Cell has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_ptr().cast::<Cell>();
                let len = bytes.len() / size_of::<Cell>();
                std::slice::from_raw_parts(ptr, len)
            }
        })
    }

    #[must_use]
    pub fn cells_mut(&mut self) -> RefMut<[Cell]> {
        let cells_offset = self.cells_offset();

        let data = self.account.data_mut();
        let data = RefMut::map(data, |d| &mut d[cells_offset..]);

        RefMut::map(data, |bytes| {
            static_assertions::assert_eq_align!(Cell, u8);
            assert_eq!(bytes.len() % size_of::<Cell>(), 0);

            // SAFETY: Cell has the same alignment as bytes
            unsafe {
                let ptr = bytes.as_mut_ptr().cast::<Cell>();
                let len = bytes.len() / size_of::<Cell>();
                std::slice::from_raw_parts_mut(ptr, len)
            }
        })
    }

    #[must_use]
    pub fn get(&self, subindex: u8) -> [u8; 32] {
        for cell in &*self.cells() {
            if cell.subindex != subindex {
                continue;
            }

            return cell.value;
        }

        [0_u8; 32]
    }

    fn get_mut(&mut self, subindex: u8) -> Option<RefMut<[u8; 32]>> {
        let cells = self.cells_mut();
        RefMut::filter_map(cells, |cells| {
            for cell in cells {
                if cell.subindex == subindex {
                    return Some(&mut cell.value);
                }
            }
            None
        })
        .ok()
    }

    fn allocate_cell(&mut self) -> Result<RefMut<Cell>> {
        let new_len = self.account.data_len() + size_of::<Cell>(); // new_len <= 8.25 kb
        self.account.reallocate(new_len, ZeroInit::Uninit)?;

        let cells = self.cells_mut();
        let filter_result = RefMut::filter_map(cells, <[Cell]>::last_mut);

        // SAFETY: We just allocated a new cell, filter_map will never fail
        // Use unwrap_unchecked instead of unwrap beacause [Cell] does not implement Debug
        Ok(unsafe { filter_result.unwrap_unchecked() })
    }

    pub fn update(&mut self, subindex: u8, value: &[u8; 32]) -> Result<()> {
        let need_revision_increment = 'revision: {
            // Try to find an existing cell with the same subindex
            if let Some(mut cell) = self.get_mut(subindex) {
                if cell.cmp(value) == Ordering::Equal {
                    // Cell already exists with the same value, do nothing
                    break 'revision false;
                }

                // Update the existing cell and increment revision
                cell.copy_from_slice(value);
                break 'revision true;
            }

            // Existing cell not found

            // If the value is zero, do not allocate a new cell
            if value.cmp(&[0_u8; 32]) == Ordering::Equal {
                break 'revision false;
            }

            // Value is not zero, allocate a new cell
            let mut cell = self.allocate_cell()?;
            cell.subindex = subindex;
            cell.value.copy_from_slice(value);

            true
        };

        if need_revision_increment {
            self.increment_revision()?;
        }

        Ok(())
    }

    #[must_use]
    pub fn revision(&self) -> u32 {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            return 0;
        }

        let header: Ref<HeaderWithRevision> = self.account.header();
        header.revision
    }

    pub fn increment_revision(&mut self) -> Result<()> {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            self.header_upgrade()?;
        }

        let mut header: RefMut<HeaderWithRevision> = self.account.header_mut();
        header.revision = header.revision.wrapping_add(1);

        Ok(())
    }
}
