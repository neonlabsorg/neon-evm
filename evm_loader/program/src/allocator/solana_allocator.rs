use std::ops::Range;
use std::slice;

use linked_list_allocator::Heap;
use solana_program::entrypoint::HEAP_START_ADDRESS;

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

const SOLANA_HEAP_RANGE: Range<usize> = Range {
    start: SOLANA_HEAP_START_ADDRESS,
    end: SOLANA_HEAP_START_ADDRESS + SOLANA_HEAP_SIZE,
};

const _: () = {
    assert!(HEAP_START_ADDRESS < (usize::MAX as u64));
    assert!((SOLANA_HEAP_START_ADDRESS % std::mem::align_of::<Heap>()) == 0);
};

#[derive(Copy, Clone)]
pub struct SolanaAllocator {
    heap: *mut Heap,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl SolanaAllocator {
    pub fn new(heap: *mut Heap) -> Self {
        Self { heap }
    }

    unsafe fn alloc_impl(&self, layout: Layout) -> Result<NonNull<u8>, ()> {
        let heap = &mut *self.heap;
        heap.allocate_first_fit(layout)
    }

    unsafe fn dealloc_impl(&self, ptr: *mut u8, layout: Layout) {
        let heap = &mut *self.heap;
        heap.deallocate(NonNull::new_unchecked(ptr), layout);
    }

    fn error_msg(&self) -> &'static str {
        let ptr = self.heap as usize;
        if SOLANA_HEAP_RANGE.contains(&ptr) {
            "Solana heap allocator out of memory"
        } else {
            "EVM Account Allocator out of memory"
        }
    }
}

unsafe impl std::alloc::GlobalAlloc for SolanaAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[allow(clippy::option_if_let_else)]
        if let Ok(non_null) = self.alloc_impl(layout) {
            non_null.as_ptr()
        } else {
            solana_program::log::sol_log(self.error_msg());
            std::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc_impl(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);

        if !ptr.is_null() {
            #[cfg(target_os = "solana")]
            solana_program::syscalls::sol_memset_(ptr, 0, layout.size() as u64);

            #[cfg(not(target_os = "solana"))]
            ptr.write_bytes(0, layout.size());
        }

        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = self.alloc(new_layout);

        if !new_ptr.is_null() {
            let copy_bytes = std::cmp::min(layout.size(), new_size);

            #[cfg(target_os = "solana")]
            solana_program::syscalls::sol_memcpy_(new_ptr, ptr, copy_bytes as u64);

            #[cfg(not(target_os = "solana"))]
            ptr.copy_to_nonoverlapping(new_ptr, copy_bytes);

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
                    solana_program::log::sol_log(self.error_msg());
                    allocator_api2::alloc::AllocError
                })
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.dealloc_impl(ptr.as_ptr(), layout);
    }
}

#[cfg(target_os = "solana")]
struct StaticAllocator {
    start_address: usize,
    heap_size: usize,
}

#[cfg(target_os = "solana")]
impl StaticAllocator {
    fn maybe_init(&self) -> SolanaAllocator {
        let heap_ptr: *mut Heap = self.start_address as *mut Heap;
        let heap = unsafe { &mut *heap_ptr };

        if heap.bottom().is_null() {
            let start = (self.start_address + std::mem::size_of::<Heap>()) as *mut u8;
            let size = self.heap_size - std::mem::size_of::<Heap>();
            unsafe { heap.init(start, size) };
        }

        SolanaAllocator { heap: heap_ptr }
    }
}

#[cfg(target_os = "solana")]
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

#[cfg(target_os = "solana")]
#[global_allocator]
static DEFAULT: StaticAllocator = StaticAllocator {
    start_address: SOLANA_HEAP_START_ADDRESS,
    heap_size: SOLANA_HEAP_SIZE,
};
