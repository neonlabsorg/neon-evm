use std::mem::{align_of, size_of};

use linked_list_allocator::Heap;
use solana_program::entrypoint::HEAP_START_ADDRESS;
use static_assertions::{const_assert, const_assert_eq};

use crate::allocator::solana::Alloc;

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

#[derive(Clone, Copy)]
pub struct SolanaAllocator;

impl Alloc for SolanaAllocator {
    fn heap() -> &'static mut Heap {
        // This is legal since all-zero is a valid `Heap`-struct representation
        const HEAP_PTR: *mut Heap = SOLANA_HEAP_START_ADDRESS as *mut Heap;
        let heap = unsafe { &mut *HEAP_PTR };

        if heap.bottom().is_null() {
            let start = (SOLANA_HEAP_START_ADDRESS + size_of::<Heap>()) as *mut u8;
            let size = SOLANA_HEAP_SIZE - size_of::<Heap>();
            unsafe { heap.init(start, size) };
        }

        heap
    }
}
