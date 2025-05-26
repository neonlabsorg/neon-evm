use std::mem::{align_of, size_of};
use std::slice;

use linked_list_allocator::Heap;
use solana_program::entrypoint::HEAP_START_ADDRESS;
use static_assertions::{const_assert, const_assert_eq};

use crate::allocator::STATE_ACCOUNT_DATA_ADDRESS;
use std::alloc::Layout;
use std::ptr::NonNull;

// Solana heap constants.
#[allow(clippy::cast_possible_truncation)] // HEAP_START_ADDRESS < usize::max
const SOLANA_HEAP_START_ADDRESS: usize = HEAP_START_ADDRESS as usize;

cfg_if::cfg_if! {
    if #[cfg(feature = "rollup")] {
        // NeonEVM under rollup is intended to be deployed with a forked version of Solana that supports such bigger heap.
        const SOLANA_HEAP_SIZE: usize = 1024 * 1024;
    } else {
        const SOLANA_HEAP_SIZE: usize = 256 * 1024;
    }
}

const_assert!(HEAP_START_ADDRESS < (usize::MAX as u64));

const_assert_eq!(SOLANA_HEAP_START_ADDRESS % align_of::<Heap>(), 0);

// Configure State/Holder Account heap: the offset of the heap object is at HEAP_OBJECT_OFFSET_PTR address.
#[allow(clippy::cast_possible_truncation)]
const HEAP_OBJECT_OFFSET_PTR: usize = STATE_ACCOUNT_DATA_ADDRESS + crate::account::HEAP_OFFSET_PTR;

#[derive(Copy, Clone)]
pub struct SolanaAllocator {
    heap: *mut Heap,
    error_msg: &'static str,
}

impl SolanaAllocator {
    pub unsafe fn from_slice(slice: &mut [u8], error_msg: &'static str) -> Result<Self, ()> {
        let mut heap = Heap::from_slice(unsafe { std::mem::transmute(slice) });
        let ptr = heap.allocate_first_fit(Layout::new::<Heap>())?;
        unsafe {
            let heap_ref: &mut Heap = &mut *ptr.as_ptr().cast::<Heap>();
            *heap_ref = heap
        }
        Ok(SolanaAllocator {
            heap: ptr.as_ptr().cast::<Heap>(),
            error_msg,
        })
    }

    #[cfg(target_os = "solana")]
    pub fn static_account_alloc() -> Self {
        let heap_object_offset_ptr = HEAP_OBJECT_OFFSET_PTR as *const usize;
        let heap_object_offset = unsafe { std::ptr::read_unaligned(heap_object_offset_ptr) };
        let heap_ptr: *mut Heap = (STATE_ACCOUNT_DATA_ADDRESS + heap_object_offset) as *mut Heap;
        // Unlike SolanaAllocator, AccountAllocator do not init account heap here.
        // It's account's responsibility to initialize it itself (likely during
        // Holder/StateAccount creation), because account knows its size and thus can
        // correctly specify heap size.

        SolanaAllocator {
            heap: heap_ptr,
            error_msg: "EVM Account Allocator out of memory",
        }
    }

    fn heap(&self) -> &mut Heap {
        unsafe { &mut *self.heap }
    }

    fn alloc_impl(&self, layout: Layout) -> Result<NonNull<u8>, ()> {
        self.heap().allocate_first_fit(layout)
    }

    fn dealloc_impl(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            self.heap().deallocate(NonNull::new_unchecked(ptr), layout);
        }
    }
}

unsafe impl std::alloc::GlobalAlloc for SolanaAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[allow(clippy::option_if_let_else)]
        if let Ok(non_null) = self.alloc_impl(layout) {
            non_null.as_ptr()
        } else {
            solana_program::log::sol_log(&self.error_msg);
            std::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc_impl(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);

        if !ptr.is_null() {
            solana_program::syscalls::sol_memset_(ptr, 0, layout.size() as u64);
        }

        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(new_layout);

        if !new_ptr.is_null() {
            let copy_bytes = std::cmp::min(layout.size(), new_size);

            solana_program::syscalls::sol_memcpy_(new_ptr, ptr, copy_bytes as u64);

            self.dealloc(ptr, layout);
        }

        new_ptr
    }
}

unsafe impl allocator_api2::alloc::Allocator for SolanaAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        unsafe {
            self.alloc_impl(layout)
                .map(|ptr| {
                    NonNull::new_unchecked(slice::from_raw_parts_mut(ptr.as_ptr(), layout.size()))
                })
                .map_err(|()| {
                    solana_program::log::sol_log(self.error_msg);
                    allocator_api2::alloc::AllocError
                })
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.dealloc_impl(ptr.as_ptr(), layout);
    }
}

struct StaticAllocator {
    start_address: usize,
    heap_size: usize,
    error_msg: &'static str,
}

impl StaticAllocator {
    fn maybe_init(&self) -> SolanaAllocator {
        let mask = std::mem::align_of::<Heap>() - 1;
        if self.start_address & mask != 0 {
            panic!("bad alignment")
        }

        let heap_ptr: *mut Heap = self.start_address as *mut Heap;
        let heap = unsafe { &mut *heap_ptr };

        if heap.bottom().is_null() {
            let start = (self.start_address + size_of::<Heap>()) as *mut u8;
            let size = self.heap_size - size_of::<Heap>();
            unsafe { heap.init(start, size) };
        }

        SolanaAllocator {
            heap: heap_ptr,
            error_msg: self.error_msg,
        }
    }
}

unsafe impl std::alloc::GlobalAlloc for StaticAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.maybe_init().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.maybe_init().dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        self.maybe_init().alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.maybe_init().realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static DEFAULT: StaticAllocator = StaticAllocator {
    start_address: SOLANA_HEAP_START_ADDRESS,
    heap_size: SOLANA_HEAP_SIZE,
    error_msg: "Solana Allocator out of memory",
};
