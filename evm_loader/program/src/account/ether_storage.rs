use std::cell::{Ref, RefMut};
use std::mem::size_of;

use super::{
    Account, AccountDispatch, AccountHeader, AccountsDB, NoHeader, ZeroInit, ACCOUNT_PREFIX_LEN,
    TAG_EMPTY, TAG_STORAGE_CELL,
};
use crate::error::{Error, Result};
use ethnum::U256;
use solana_program::account_info::AccountInfo;
use solana_program::{pubkey::Pubkey, rent::Rent};

#[derive(Copy, Clone)]
pub struct StorageCellAddress {
    base: Pubkey,
    seed: [u8; 32],
    pubkey: Pubkey,
}

impl StorageCellAddress {
    #[must_use]
    fn make_seed(index: &U256) -> [u8; 32] {
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
    pub fn new(program_id: &Pubkey, base: &Pubkey, index: &U256) -> Self {
        let seed_buffer = Self::make_seed(index);
        let seed = unsafe { std::str::from_utf8_unchecked(&seed_buffer) };

        let pubkey = Pubkey::create_with_seed(base, seed, program_id).unwrap();

        Self {
            base: *base,
            seed: seed_buffer,
            pubkey,
        }
    }

    #[must_use]
    pub fn seed(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.seed) }
    }

    #[must_use]
    pub fn pubkey(&self) -> &Pubkey {
        &self.pubkey
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

    pub fn create(
        address: StorageCellAddress,
        allocate_cells: usize,
        accounts: &AccountsDB<'a>,
        signer_seeds: &[&[u8]],
        rent: &Rent,
    ) -> Result<Self> {
        let base_account = accounts.get(&address.base);
        let cell_account = accounts.get(&address.pubkey);

        assert!(allocate_cells <= u8::MAX.into());
        let space = Self::required_account_size(allocate_cells);

        let system = accounts.system();

        system.create_account_with_seed(
            &crate::ID,
            accounts.operator(),
            base_account,
            signer_seeds,
            cell_account,
            address.seed(),
            space,
            rent,
        )?;

        let account = cell_account.clone().into();
        Self::initialize(account, crate::ID)
    }

    pub fn initialize(mut account: Account<'a>, program_id: Pubkey) -> Result<Self> {
        assert!(account.validate_tag(program_id, TAG_EMPTY).is_ok());

        account.init_tag(TAG_STORAGE_CELL, Header::VERSION)?;
        {
            let mut header: RefMut<Header> = account.header_mut();
            header.revision = 1; // TODO: set it to zero after AccountStorage changes
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

    fn header_upgrade(&mut self, rent: &Rent, db: &AccountsDB<'a>) -> Result<()> {
        match self.account.header_version() {
            0 | 1 => {
                self.account.expand_header::<NoHeader, Header>(rent, db)?;
            }
            HeaderWithRevision::VERSION => {
                self.account
                    .expand_header::<HeaderWithRevision, Header>(rent, db)?;
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

    pub fn update(&mut self, subindex: u8, value: &[u8; 32]) -> Result<()> {
        // todo: if value is zero - destroy cell

        for cell in &mut *self.cells_mut() {
            if cell.subindex != subindex {
                continue;
            }

            cell.value.copy_from_slice(value);
            return Ok(());
        }

        if value == &[0u8; 32] {
            return Ok(());
        }

        let new_len = self.account.data_len() + size_of::<Cell>(); // new_len <= 8.25 kb
        self.account.reallocate(new_len, ZeroInit::Uninit)?;

        let mut cells = self.cells_mut();

        let last_cell = cells.last_mut().unwrap();
        last_cell.subindex = subindex;
        last_cell.value.copy_from_slice(value);

        Ok(())
    }

    pub fn sync_lamports(&mut self, rent: &Rent, accounts: &AccountsDB<'a>) -> Result<()> {
        self.account.sync_lamports(rent, accounts)
    }

    #[must_use]
    pub fn revision(&self) -> u32 {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            return 0;
        }

        let header: Ref<HeaderWithRevision> = self.account.header();
        header.revision
    }

    pub fn increment_revision(&mut self, rent: &Rent, db: &AccountsDB<'a>) -> Result<()> {
        if self.account.header_version() < HeaderWithRevision::VERSION {
            self.header_upgrade(rent, db)?;
        }

        let mut header: RefMut<HeaderWithRevision> = self.account.header_mut();
        header.revision = header.revision.wrapping_add(1);

        Ok(())
    }
}
