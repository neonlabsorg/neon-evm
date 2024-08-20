use std::alloc::Layout;
use std::ptr::NonNull;
use std::slice;

use linked_list_allocator::Heap;

use crate::allocator::solana::Alloc;
use crate::allocator::STATE_ACCOUNT_DATA_ADDRESS;

#[derive(Clone, Copy)]
pub struct AccountAllocator;

// Configure State/Holder Account heap: the offset of the heap object is at HEAP_OBJECT_OFFSET_PTR address.
#[allow(clippy::cast_possible_truncation)]
const HEAP_OBJECT_OFFSET_PTR: usize = STATE_ACCOUNT_DATA_ADDRESS + crate::account::HEAP_OFFSET_PTR;

impl Alloc for AccountAllocator {
    fn heap() -> &'static mut Heap {
        let heap_object_offset_ptr = HEAP_OBJECT_OFFSET_PTR as *const usize;
        let heap_object_offset = unsafe { std::ptr::read_unaligned(heap_object_offset_ptr) };
        let heap_ptr: *mut Heap = (STATE_ACCOUNT_DATA_ADDRESS + heap_object_offset) as *mut Heap;
        let heap = unsafe { &mut *heap_ptr };
        // Unlike SolanaAllocator, AccountAllocator do not init account heap here.
        // It's account's responsibility to initialize it itself (likely during
        // Holder/StateAccount creation), because account knows its size and thus can
        // correctly specify heap size.

        heap
    }
}

unsafe impl allocator_api2::alloc::Allocator for AccountAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        unsafe {
            Self::alloc_impl(layout)
                .map(|ptr| {
                    NonNull::new_unchecked(slice::from_raw_parts_mut(ptr.as_ptr(), layout.size()))
                })
                .map_err(|()| allocator_api2::alloc::AllocError)
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        Self::dealloc_impl(ptr.as_ptr(), layout);
    }
}
