#![allow(clippy::await_holding_refcell_ref)]

use std::cell::{Ref, RefMut};
use std::mem::size_of;
use std::ops::Range;

use crate::account::state_root::AccountsStatus;
use crate::allocator::StateAllocator;
use crate::debug::log_data;
use crate::error::{Error, Result};
#[cfg(not(target_os = "platform"))]
use crate::executor::BlockParams;
use crate::platform::Platform;
use crate::types::{EncodedTransaction, Transaction};

use linked_list_allocator::Heap;
use maybe_async::maybe_async;
use solana_program::keccak;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use super::state_root::Root;
use super::{
    AccountDispatch, AccountHeader, StateFinalizedAccount, TransactionTree, ACCOUNT_PREFIX_LEN,
    TAG_SCHEDULED_STATE_CANCELLED, TAG_SCHEDULED_STATE_FINALIZED, TAG_STATE,
};

// Account Layout
// ----------------
// Header
//   - version_signature: 40 bytes
//   - transaction_len: 8 bytes
// ---------------
// Transaction Hash: 32 bytes
// Transaction RLP: `transaction_len` bytes
// ---------------
// alignment to `align_of::<Root>()`
// ---------------
// Root
//   - plain_data: PlainData
//   - the rest of the fields
// ---------------
// alignment to `align_of::<Heap>()`
// ---------------
// Heap
// ---------------

type VersionSignature = [u8; 40];

#[repr(C, packed)]
struct Offsets {
    pub header: usize,
    pub tx_hash: usize,
    pub tx_rlp: usize,
    pub tx_rlp_len: usize,
    pub root: usize,
    pub heap_object: usize,
}

impl Offsets {
    pub fn transaction(&self) -> (Range<usize>, Range<usize>) {
        let hash = self.tx_hash..(self.tx_hash + size_of::<keccak::Hash>());
        let rlp = self.tx_rlp..(self.tx_rlp + self.tx_rlp_len);

        (hash, rlp)
    }
}

#[repr(C, packed)]
struct Header {
    pub version_signature: VersionSignature,
    pub owner: Pubkey,
    pub account_memory_address: u64,
    pub offsets: Offsets,
}

impl Header {
    fn valid_version_signature() -> VersionSignature {
        let mut result: VersionSignature = [0; 40];
        let state = env!("NEON_REVISION").as_bytes();
        result[..state.len()].copy_from_slice(state);
        result
    }
}

impl AccountHeader for Header {
    const VERSION: u8 = 2;
}

pub struct StateAccount<'sol> {
    account: AccountInfo<'sol>,
    tag: u8,
}

#[maybe_async(?Send)]
impl<'a> StateAccount<'a> {
    #[must_use]
    pub fn into_account(self) -> AccountInfo<'a> {
        self.account
    }

    fn validate_tag(account_key: Pubkey, tag: u8) -> Result<()> {
        if tag == TAG_STATE
            || tag == TAG_SCHEDULED_STATE_FINALIZED
            || tag == TAG_SCHEDULED_STATE_CANCELLED
        {
            Ok(())
        } else {
            Err(Error::StorageAccountInvalidTag(account_key, tag))
        }
    }

    #[must_use]
    fn header_size(&self) -> usize {
        match self.account.header_version() {
            Header::VERSION => size_of::<Header>(),
            v => panic_with_error!(Error::AccountInvalidHeader(self.pubkey(), v)),
        }
    }

    #[must_use]
    pub fn pubkey(&self) -> Pubkey {
        *self.account.key
    }

    pub fn from_account_info(program_id: Pubkey, account_info: &AccountInfo<'a>) -> Result<Self> {
        let account = account_info.clone();
        Self::from_account(program_id, account)
    }

    pub fn from_account(program_id: Pubkey, account: AccountInfo<'a>) -> Result<Self> {
        let tag = account.tag(program_id)?;
        Self::validate_tag(account.pubkey(), tag)?;

        Ok(Self { account, tag })
    }

    pub async fn new(
        account: AccountInfo<'a>,
        owner: Pubkey,
        transaction: EncodedTransaction<'_>,
        platform: &mut (impl Platform<'a> + 'a),
    ) -> Result<Self> {
        let (state, _) = Self::new_inner(account, owner, transaction, platform, None).await?;
        Ok(state)
    }

    pub async fn new_with_tree(
        account: AccountInfo<'a>,
        owner: Pubkey,
        transaction: EncodedTransaction<'_>,
        platform: &mut (impl Platform<'a> + 'a),
        tree: &mut TransactionTree<'a>,
    ) -> Result<(Self, Transaction)> {
        Self::new_inner(account, owner, transaction, platform, Some(tree)).await
    }

    async fn new_inner(
        mut account: AccountInfo<'a>,
        owner: Pubkey,
        transaction: EncodedTransaction<'_>,
        platform: &mut (impl Platform<'a> + 'a),
        tree: Option<&mut TransactionTree<'a>>,
    ) -> Result<(Self, Transaction)> {
        account.init_tag(TAG_STATE, Header::VERSION)?;
        let mut state = Self {
            account,
            tag: TAG_STATE,
        };
        let transaction = unsafe {
            state.initialize_header(owner, &transaction);
            state.initialize_transaction(&transaction);
            state.initialize_heap();
            state.initialize_root(transaction, tree, platform).await?
        };

        Ok((state, transaction))
    }

    pub async fn restore(
        account: AccountInfo<'a>,
        platform: &mut (impl Platform<'a> + 'a),
    ) -> Result<Self> {
        let mut state = Self::from_account(platform.program_id(), account)?;
        state.assert_memory_address();

        let status = state.validate_accounts_status(platform).await?;
        if status == AccountsStatus::NeedRestart {
            log_data(&[b"RESET"]);
            unsafe {
                state.initialize_heap();
                state.reset_root(platform).await?;
                state.reset_header_signature();
            }
        }

        Ok(state)
    }

    #[inline]
    #[track_caller]
    fn assert_memory_address(&self) {
        // Validate that the SVM memory address is the same as before
        // Could fail if the account is not first in the instruction or because of the changes to the SVM
        // We can't use the Heap inside of the account if this is not valid
        assert_eq!(self.stored_memory_address(), self.account.memory_address());
    }

    async fn validate_accounts_status(
        &self,
        platform: &impl Platform<'a>,
    ) -> Result<AccountsStatus> {
        let version_signature = {
            let header: Ref<Header> = self.account.header();
            header.version_signature
        };
        if version_signature != Header::valid_version_signature() {
            return Ok(AccountsStatus::NeedRestart);
        }

        // Heap is valid, we can access the root and validate accounts
        let root = self.root();
        root.validate_accounts(platform).await
    }

    pub fn cancel(self) -> Result<()> {
        let is_scheduled_transaction = self.root().is_scheduled_transaction();

        if is_scheduled_transaction {
            self.finalize_impl(Some(TAG_SCHEDULED_STATE_CANCELLED))
        } else {
            self.finalize_impl(None)
        }
    }

    pub fn finalize(self) -> Result<()> {
        let is_scheduled_transaction = self.root().is_scheduled_transaction();

        if is_scheduled_transaction {
            self.finalize_impl(Some(TAG_SCHEDULED_STATE_FINALIZED))
        } else {
            self.finalize_impl(None)
        }
    }

    fn finalize_impl(mut self, transition_tag: Option<u8>) -> Result<()> {
        if self.tag != TAG_STATE {
            return Err(Error::AccountInvalidTag(self.pubkey(), self.tag));
        }

        if let Some(transition_tag) = transition_tag {
            // Change the tag, leave all the data unchanged.
            self.account.replace_tag(transition_tag)?;
        } else {
            StateFinalizedAccount::convert_from_state(self)?;
        }

        Ok(())
    }

    pub fn finalize_scheduled_tx(self) -> Result<()> {
        let is_finalized = self.tag == TAG_SCHEDULED_STATE_FINALIZED;
        let is_canceled = self.tag == TAG_SCHEDULED_STATE_CANCELLED;
        if !(is_finalized || is_canceled) {
            return Err(Error::StorageAccountInvalidTag(self.pubkey(), self.tag));
        }

        StateFinalizedAccount::convert_from_state(self)?;

        Ok(())
    }

    #[must_use]
    fn align_offset<T: Sized>(&self, offset: usize) -> usize {
        let min_required_data_len = offset + align_of::<T>() + size_of::<T>();
        assert!(self.account.data_len() > min_required_data_len);

        let data_ptr = self.account.data().as_ptr();

        // SAFETY: Entire range in between `data_ptr` and `data_ptr.add(offset)` in bound of the account data
        let object_ptr = unsafe { data_ptr.add(offset) };
        let alignment = object_ptr.align_offset(align_of::<T>());

        offset + alignment
    }

    #[must_use]
    fn calculate_offsets(&self, transaction_len: usize) -> Offsets {
        let header = ACCOUNT_PREFIX_LEN;
        let header_len = self.header_size();

        let tx_hash = header + header_len;
        let tx_hash_len = size_of::<keccak::Hash>();

        let tx_rlp = tx_hash + tx_hash_len;
        let tx_rlp_len = transaction_len;

        let root = self.align_offset::<Root>(tx_rlp + tx_rlp_len);
        let root_len = size_of::<Root>();

        let heap_object = self.align_offset::<Heap>(root + root_len);

        Offsets {
            header,
            tx_hash,
            tx_rlp,
            tx_rlp_len,
            root,
            heap_object,
        }
    }

    fn offsets(&self) -> Ref<Offsets> {
        let header: Ref<Header> = self.account.header();
        Ref::map(header, |h| &h.offsets)
    }

    #[must_use]
    fn stored_memory_address(&self) -> u64 {
        let header: Ref<Header> = self.account.header();
        header.account_memory_address
    }

    #[must_use]
    pub fn owner(&self) -> Pubkey {
        let header: Ref<Header> = self.account.header();
        header.owner
    }

    #[must_use]
    pub fn transaction(&self) -> EncodedTransaction<'_> {
        let (hash_range, rlp_range) = self.offsets().transaction();

        let data = self.account.data();

        let hash = {
            let hash_bytes = &data[hash_range];
            keccak::Hash(hash_bytes.try_into().unwrap())
        };
        let rlp = Ref::map(data, |d| &d[rlp_range]);

        EncodedTransaction::Ref(rlp, hash)
    }

    #[must_use]
    pub fn transaction_hash(&self) -> keccak::Hash {
        let (hash_range, _) = self.offsets().transaction();
        let data = self.account.data();

        let hash_bytes = &data[hash_range];
        keccak::Hash(hash_bytes.try_into().unwrap())
    }

    #[must_use]
    pub fn root(&self) -> Ref<Root> {
        let offset = self.offsets().root;
        self.account.section(offset)
    }

    #[must_use]
    pub fn root_mut(&mut self) -> RefMut<Root> {
        let offset = self.offsets().root;
        self.account.section_mut(offset)
    }

    /// SAFETY: It's a caller responsibility to ensure that the heap is initialized
    unsafe fn heap(&mut self) -> *mut Heap {
        let offset = self.offsets().heap_object;

        let ptr: *mut Heap = self.account.data_mut_ptr(offset).cast();
        assert!(!ptr.is_null() && ptr.is_aligned());

        ptr
    }

    /// SAFETY: This functions should be called only once
    unsafe fn initialize_header(&mut self, owner: Pubkey, transaction: &EncodedTransaction<'_>) {
        let account_memory_address = self.account.memory_address();
        let offsets = self.calculate_offsets(transaction.rlp_len());

        let mut header_section = self.account.header_mut_uninit::<Header>();
        header_section.write(Header {
            version_signature: Header::valid_version_signature(),
            owner,
            account_memory_address,
            offsets,
        });
    }

    /// SAFETY: This functions should be called with previously valid header
    unsafe fn reset_header_signature(&mut self) {
        let mut header: RefMut<Header> = self.account.header_mut();
        header.version_signature = Header::valid_version_signature();
    }

    /// SAFETY: This functions should be called only once after `initialize_header`
    unsafe fn initialize_transaction(&mut self, transaction: &EncodedTransaction<'_>) {
        let (hash_range, rlp_range) = self.offsets().transaction();

        let keccak::Hash(hash) = transaction.hash();
        let rlp = transaction.rlp();

        let mut data = self.account.data_mut();
        data[hash_range].copy_from_slice(hash);
        data[rlp_range].copy_from_slice(rlp);
    }

    /// SAFETY: This functions should be called only once after `initialize_transaction`
    unsafe fn initialize_heap(&mut self) {
        let heap_object_offset = self.offsets().heap_object;

        let heap_bottom_offset = heap_object_offset + size_of::<Heap>();
        assert!(self.account.data_len() > heap_bottom_offset);

        let heap_bottom = unsafe { self.account.data_mut_ptr(heap_bottom_offset) };
        let heap_size = self.account.data_len() - heap_bottom_offset;

        let mut heap_section = self.account.section_mut_uninit::<Heap>(heap_object_offset);
        heap_section.write(unsafe {
            // SAFETY: The bottom pointer is valid and [heap_bottom, heap_bottom + heap_size) range is in bound of the account data
            Heap::new(heap_bottom, heap_size)
        });
    }

    /// SAFETY: This functions should be called only once after `initialize_heap`
    async unsafe fn initialize_root(
        &mut self,
        encoded_transaction: EncodedTransaction<'_>,
        tree: Option<&mut TransactionTree<'a>>,
        platform: &mut (impl Platform<'a> + 'a),
    ) -> Result<Transaction> {
        let allocator = StateAllocator::new(self.heap());

        let transaction = encoded_transaction.decode()?;
        let origin = transaction.recover_caller_address()?;

        transaction
            .validate(origin, platform, tree.as_deref())
            .await?;

        let offset = self.offsets().root;

        let mut root_section = self.account.section_mut_uninit::<Root>(offset);
        root_section.write(Root::new(&transaction, origin, tree, platform, allocator).await?);

        Ok(transaction)
    }

    /// SAFETY: This functions should be called only once after `initialize_heap` and with previously valid root
    async unsafe fn reset_root(&mut self, platform: &mut (impl Platform<'a> + 'a)) -> Result<()> {
        let allocator = StateAllocator::new(self.heap());

        let tx = self.transaction().decode()?;

        let offset = self.offsets().root;

        let root = {
            // Old Root is not valid. Reading from it may cause undefined behavior.
            // Can only be used to create a new one.
            let old_root = self.account.section::<Root>(offset);
            old_root.new_after_reset(&tx, platform, allocator).await?
        };

        let mut root_section = self.account.section_mut_uninit::<Root>(offset);
        root_section.write(root);

        Ok(())
    }
}

#[cfg(not(target_os = "platform"))]
type StateAccountCoreApiView = (
    super::state_root::PlainData,
    Option<BlockParams>,
    Vec<Pubkey>,
    Vec<u8>, //tx rlp
);

#[cfg(not(target_os = "platform"))]
impl StateAccount<'_> {
    pub fn get_state_account_view(&self) -> Result<StateAccountCoreApiView> {
        use super::state_root::AccountRevision;
        use super::state_root::PlainData;
        use crate::types::read_raw_utils;

        let platform_memory_address: isize = self.stored_memory_address().try_into()?;
        let local_memory_address: isize = self.account.memory_address().try_into()?;
        let memory_delta = local_memory_address - platform_memory_address;

        let transaction = self.transaction().rlp().to_vec();

        let root = self.root();
        let data = root.plain_data;

        if data.layout_version != PlainData::layout_version() {
            // we don't have a reliable way to reconstruct revisions
            return Ok((data, None, Vec::new(), transaction));
        }

        let block_params = root.executor_state.inhereted_block_params;

        // SAFETY: It is not safe at all
        // `root.revisions` pointers are allocated in a different address space
        // we do the conversion between the platform memory addresses and the local memory addresses
        let revisions: Vec<(Pubkey, AccountRevision)> = unsafe {
            let ptr = &raw const root.revisions;
            read_raw_utils::read_vec(ptr.cast(), memory_delta)
        };
        let accounts: Vec<Pubkey> = revisions.into_iter().map(|(key, _)| key).collect();

        Ok((data, block_params, accounts, transaction))
    }
}
