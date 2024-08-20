use linked_list_allocator::Heap;
use std::alloc::Layout;
use std::ptr::NonNull;

use solana_allocator::SolanaAllocator;
use state_account_allocator::AccountAllocator;

pub mod solana_allocator;
pub mod state_account_allocator;

trait Alloc {
    fn heap() -> &'static mut Heap;

    fn alloc_impl(layout: Layout) -> Result<NonNull<u8>, ()> {
        Self::heap().allocate_first_fit(layout)
    }

    fn dealloc_impl(ptr: *mut u8, layout: Layout) {
        unsafe {
            Self::heap().deallocate(NonNull::new_unchecked(ptr), layout);
        }
    }
}

macro_rules! impl_global_alloc {
    ($t:ty, $err:expr) => {
        unsafe impl std::alloc::GlobalAlloc for $t {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                #[allow(clippy::option_if_let_else)]
                if let Ok(non_null) = Self::alloc_impl(layout) {
                    non_null.as_ptr()
                } else {
                    solana_program::log::sol_log($err);
                    std::ptr::null_mut()
                }
            }

            unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                Self::dealloc_impl(ptr, layout);
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
    };
}

impl_global_alloc!(SolanaAllocator, "Solana Allocator out of memory");

impl_global_alloc!(AccountAllocator, "EVM Account Allocator out of memory");
