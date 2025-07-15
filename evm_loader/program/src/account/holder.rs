use linked_list_allocator::Heap;
use solana_program::account_info::AccountInfo;
use solana_program::keccak;
use solana_program::pubkey::Pubkey;
use std::alloc::Layout;
use std::cell::{Ref, RefMut};
use std::mem::size_of;

use crate::account::AccountDispatch;
use crate::account::TAG_STATE_FINALIZED;
use crate::allocator::StateAllocator;
use crate::error::{Error, Result};
use crate::types::EncodedTransaction;

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

pub struct Holder<'sol> {
    account: AccountInfo<'sol>,
}

impl<'sol> Holder<'sol> {
    #[must_use]
    pub fn into_account(self) -> AccountInfo<'sol> {
        self.account
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        self.account.pubkey()
    }

    pub fn from_account_info(program_id: Pubkey, account_info: &AccountInfo<'sol>) -> Result<Self> {
        let account = account_info.clone();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, mut account: AccountInfo<'sol>) -> Result<Self> {
        match account.tag(program_id)? {
            TAG_STATE_FINALIZED => {
                account.init_tag(TAG_HOLDER, Header::VERSION)?;

                let mut holder = Self { account };
                holder.clear();

                Ok(holder)
            }
            TAG_HOLDER => Ok(Self { account }),
            _ => Err(Error::AccountInvalidTag(*account.key, TAG_HOLDER)),
        }
    }

    pub fn create(
        program_id: Pubkey,
        mut account: AccountInfo<'sol>,
        seed: &str,
        operator: &Operator,
    ) -> Result<Self> {
        if account.owner != &program_id {
            return Err(Error::AccountInvalidOwner(account.pubkey(), program_id));
        }

        let key = Pubkey::create_with_seed(operator.key, seed, &program_id)?;
        if &key != account.key {
            return Err(Error::AccountInvalidKey(*account.key, key));
        }

        account.validate_tag(program_id, TAG_EMPTY)?;
        account.init_tag(TAG_HOLDER, Header::VERSION)?;

        let mut holder = Self::from_account(program_id, account)?;
        holder.update(|h| h.owner = *operator.key);
        holder.clear();

        Ok(holder)
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Header),
    {
        let mut header: RefMut<Header> = self.account.header_mut();
        f(&mut header);
    }

    fn header_size(&self) -> usize {
        match self.account.header_version() {
            0 => size_of::<Header>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    fn buffer_offset(&self) -> usize {
        ACCOUNT_PREFIX_LEN + self.header_size()
    }

    fn buffer(&self) -> Ref<[u8]> {
        let offset = self.buffer_offset();

        let data = self.account.data();
        Ref::map(data, |d| &d[offset..])
    }

    fn buffer_mut(&mut self) -> RefMut<[u8]> {
        let offset = self.buffer_offset();

        let data = self.account.data_mut();
        RefMut::map(data, |d| &mut d[offset..])
    }

    pub fn clear(&mut self) {
        let mut header: RefMut<Header> = self.account.header_mut();
        header.transaction_hash.fill(0);
        header.transaction_len = 0;
    }

    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> Result<()> {
        let begin = offset;
        let end = offset
            .checked_add(bytes.len())
            .ok_or(Error::IntegerOverflow)?;

        {
            let mut header: RefMut<Header> = self.account.header_mut();
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
        let header: Ref<Header> = self.account.header();
        header.transaction_len
    }

    pub fn transaction(&self) -> Result<EncodedTransaction> {
        let stored_hash = self.transaction_hash();

        let transaction = {
            let len = self.transaction_len();
            let buffer = self.buffer();
            Ref::map(buffer, |b| &b[..len])
        };
        let transaction_hash = keccak::hash(&transaction);

        if stored_hash != transaction_hash {
            return Err(Error::HolderInvalidHash(stored_hash.0, transaction_hash.0));
        }

        Ok(EncodedTransaction::Ref(transaction, transaction_hash))
    }

    #[must_use]
    pub fn transaction_hash(&self) -> keccak::Hash {
        let header: Ref<Header> = self.account.header();
        keccak::Hash(header.transaction_hash)
    }

    pub fn update_transaction_hash(&mut self, hash: [u8; 32]) {
        if self.transaction_hash().to_bytes() == hash {
            return;
        }

        self.clear();
        self.update(|h| h.transaction_hash = hash);
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header: Ref<Header> = self.account.header();
        header.owner
    }

    pub fn validate(&self, operator: &Operator) -> Result<()> {
        if &self.owner() != operator.key {
            return Err(Error::HolderInvalidOwner(self.owner(), *operator.key));
        }

        Ok(())
    }

    #[must_use]
    pub fn into_allocator(mut self) -> StateAllocator {
        let mut buffer = self.buffer_mut();

        let heap_bottom = buffer.as_mut_ptr();
        let heap_size = buffer.len();

        let heap = unsafe {
            let mut heap = Heap::new(heap_bottom, heap_size);

            let layout = Layout::new::<Heap>();
            let Ok(ptr) = heap.allocate_first_fit(layout) else {
                std::alloc::handle_alloc_error(layout)
            };

            let ptr = ptr.cast::<Heap>();
            ptr.write(heap);

            ptr
        };

        StateAllocator::new(heap.as_ptr())
    }
}
