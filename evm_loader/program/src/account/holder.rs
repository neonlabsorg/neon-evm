use linked_list_allocator::Heap;
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use static_assertions::const_assert;
use std::cell::{Ref, RefMut};
use std::mem::{align_of, size_of};
use std::ptr::write_unaligned;

use crate::account::TAG_STATE_FINALIZED;
use crate::allocator::STATE_ACCOUNT_DATA_ADDRESS;
use crate::error::{Error, Result};
use crate::types::Transaction;

use super::{AccountHeader, Operator, ACCOUNT_PREFIX_LEN, TAG_EMPTY, TAG_HOLDER};

/// Ethereum holder data account
#[repr(C, packed)]
pub struct Header {
    pub owner: Pubkey,
    pub transaction_hash: [u8; 32],
    pub transaction_len: usize,
}

impl AccountHeader for Header {
    const VERSION: u8 = 0;
}

pub struct Holder<'a> {
    account: AccountInfo<'a>,
}

// Offset of the memory cell that denotes pointer to the heap from the start of the header.
const HEAP_PTR_OFFSET: usize = 96;
const HEADER_OFFSET: usize = ACCOUNT_PREFIX_LEN;
pub const BUFFER_OFFSET: usize = HEADER_OFFSET + HEAP_PTR_OFFSET + size_of::<usize>();

pub const HEAP_OFFSET_OFFSET: usize = HEADER_OFFSET + HEAP_PTR_OFFSET;
// State Account Header, State Finalized Header and Holder Account Header should have a shared
// and fixed memory cell that denotes the offset of the persistent heap.
// The following aserts checks that State Account Header and State Finalized Header does not overlap
// with `heap_offset` memory cell, so writes to State Account Header do not override it.
const_assert!(HEAP_PTR_OFFSET >= size_of::<Header>());
const_assert!(HEAP_PTR_OFFSET >= size_of::<crate::account::state::Header>());
const_assert!(HEAP_PTR_OFFSET >= size_of::<crate::account::state_finalized::Header>());

impl<'a> Holder<'a> {
    pub fn from_account(program_id: &Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        match super::tag(program_id, &account)? {
            TAG_STATE_FINALIZED => {
                super::set_tag(program_id, &account, TAG_HOLDER, Header::VERSION)?;

                let mut holder = Self { account };
                holder.clear();

                Ok(holder)
            }
            TAG_HOLDER => Ok(Self { account }),
            _ => Err(Error::AccountInvalidTag(*account.key, TAG_HOLDER)),
        }
    }

    pub fn create(
        program_id: &Pubkey,
        account: AccountInfo<'a>,
        seed: &str,
        operator: &Operator,
    ) -> Result<Self> {
        if account.owner != program_id {
            return Err(Error::AccountInvalidOwner(*account.key, *program_id));
        }

        let key = Pubkey::create_with_seed(operator.key, seed, program_id)?;
        if &key != account.key {
            return Err(Error::AccountInvalidKey(*account.key, key));
        }

        super::validate_tag(program_id, &account, TAG_EMPTY)?;
        super::set_tag(&crate::ID, &account, TAG_HOLDER, Header::VERSION)?;

        let mut holder = Self::from_account(program_id, account)?;
        holder.header_mut().owner = *operator.key;
        holder.clear();

        Ok(holder)
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Header),
    {
        let mut header = self.header_mut();
        f(&mut header);
    }

    fn header(&self) -> Ref<Header> {
        super::section(&self.account, HEADER_OFFSET)
    }

    fn header_mut(&mut self) -> RefMut<Header> {
        super::section_mut(&self.account, HEADER_OFFSET)
    }

    fn buffer(&self) -> Ref<[u8]> {
        let data = self.account.data.borrow();
        Ref::map(data, |d| &d[BUFFER_OFFSET..])
    }

    fn buffer_mut(&mut self) -> RefMut<[u8]> {
        let data = self.account.data.borrow_mut();
        RefMut::map(data, |d| &mut d[BUFFER_OFFSET..])
    }

    pub fn clear(&mut self) {
        {
            let mut header = self.header_mut();
            header.transaction_hash.fill(0);
            header.transaction_len = 0;
        }
        // Clear the heap ptr.
        Self::write_heap_offset(&self.account, 0);
        {
            let mut buffer = self.buffer_mut();
            buffer.fill(0);
        }
    }

    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<()> {
        let begin = offset;
        let end = offset
            .checked_add(bytes.len())
            .ok_or(Error::IntegerOverflow)?;

        {
            let mut header = self.header_mut();
            header.transaction_len = std::cmp::max(header.transaction_len, end);
        }
        {
            let mut buffer = self.buffer_mut();
            let Some(buffer) = buffer.get_mut(begin..end) else {
                return Err(Error::HolderInsufficientSize(buffer.len(), end));
            };

            buffer.copy_from_slice(bytes);
        }

        Ok(())
    }

    #[must_use]
    pub fn transaction_len(&self) -> usize {
        self.header().transaction_len
    }

    #[must_use]
    pub fn transaction(&self) -> Ref<[u8]> {
        let len = self.transaction_len();

        let buffer = self.buffer();
        Ref::map(buffer, |b| &b[..len])
    }

    #[must_use]
    pub fn transaction_hash(&self) -> [u8; 32] {
        self.header().transaction_hash
    }

    pub fn update_transaction_hash(&mut self, hash: [u8; 32]) {
        if self.transaction_hash() == hash {
            return;
        }

        self.clear();
        self.header_mut().transaction_hash = hash;
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        self.header().owner
    }

    pub fn validate_owner(&self, operator: &Operator) -> Result<()> {
        if &self.owner() != operator.key {
            return Err(Error::HolderInvalidOwner(self.owner(), *operator.key));
        }

        Ok(())
    }

    pub fn validate_transaction(&self, trx: &Transaction) -> Result<()> {
        if self.transaction_hash() != trx.hash() {
            return Err(Error::HolderInvalidHash(
                self.transaction_hash(),
                trx.hash(),
            ));
        }

        Ok(())
    }

    /// Initializes the heap using the whole account data space.
    /// Also, writes the offset of the heap object into the separate field in the header.
    /// After this, the persistent objects can be allocated into the account data.
    pub fn init_heap(&mut self, transaction_offset: usize) -> Result<()> {
        // For this case, the account.owner is already validated to be equal to program id.
        Self::init_holder_heap(self.account.owner, &mut self.account, transaction_offset)
    }

    /// Associated function, see `fn init_heap`.
    pub fn init_holder_heap(
        program_id: &Pubkey,
        account: &mut AccountInfo,
        transaction_offset: usize,
    ) -> Result<()> {
        // Validation: check that the passed account is a variant of Holder: Holder, State or StateFinalized.
        // An additional owner check is happening inside the tag.
        let tag = crate::account::tag(program_id, account)?;
        assert!(
            tag == TAG_HOLDER || tag == crate::account::TAG_STATE || tag == TAG_STATE_FINALIZED
        );

        let data_ptr = account.data.borrow().as_ptr();
        // Validation: the Holder Account used as a persistent heap, must be first in the account list.
        assert_eq!(data_ptr as usize, STATE_ACCOUNT_DATA_ADDRESS);

        // Calculate the actual aligned heap object ptr and its offset.
        let (heap_ptr, heap_object_offset) = {
            // Locate heap object into the buffer with offset no less than min_heap_object_offset.
            let mut heap_object_offset = BUFFER_OFFSET + transaction_offset;
            let mut heap_ptr = data_ptr.wrapping_add(heap_object_offset);

            // Calculate alignment and offset the heap pointer.
            let alignment = heap_ptr.align_offset(align_of::<Heap>());
            heap_ptr = heap_ptr.wrapping_add(alignment);
            heap_object_offset += alignment;
            // Validation: double check the alignment.
            assert_eq!(heap_ptr.align_offset(align_of::<Heap>()), 0);

            (heap_ptr, heap_object_offset)
        };

        // Initialize the heap.
        let heap_ptr = heap_ptr.cast_mut();
        unsafe {
            // First, zero out underlying bytes of the future heap representation.
            heap_ptr.write_bytes(0, size_of::<Heap>());
            // Calculate the bottom of the heap, right after the Heap object.
            let heap_bottom = heap_ptr.add(size_of::<Heap>());

            // Size of heap is equal to account data length minus the length of prefix.
            let heap_size = account
                .data_len()
                .saturating_sub(heap_object_offset + size_of::<Heap>());
            // Validation: check that heap object is within the account data.
            assert!(heap_size > 0);

            // Cast to reference and init.
            // Zeroed memory is a valid representation of the Heap and hence we can safely do it.
            // That's a safety reason we zeroed the memory above.
            #[allow(clippy::cast_ptr_alignment)]
            let heap = &mut *(heap_ptr.cast::<Heap>());
            heap.init(heap_bottom, heap_size);
        };

        // Write the actual heap offset into the header. This memory cell is used by the allocator.
        Self::write_heap_offset(account, heap_object_offset);

        Ok(())
    }

    /// # Safety
    /// Writes the offset of the heap object to a special memory cell.
    fn write_heap_offset(account: &AccountInfo<'_>, offset: usize) {
        #[allow(clippy::cast_ptr_alignment)]
        let heap_offset_memcell = account
            .data
            .borrow_mut()
            .as_mut_ptr()
            .wrapping_add(HEAP_OFFSET_OFFSET)
            .cast::<usize>();
        unsafe {
            write_unaligned(heap_offset_memcell, offset);
        }
    }

    /// # Safety
    /// Permanently deletes Holder account and all data in it
    pub unsafe fn suicide(self, operator: &Operator) {
        crate::account::delete(&self.account, operator);
    }
}
