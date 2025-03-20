use solana_program::slot_history::Slot;
use solana_program::sysvar::slot_hashes::PodSlotHash;

use std::{borrow::BorrowMut, cmp::Ordering};

trait SlotHashesProvider {
    fn fill_slot_hash_slice(&self, data: &mut [u8], sz: usize, offset: usize);

    fn get_slot_hash_slice<const SZ: usize>(&self, offset: usize) -> [u8; SZ] {
        let mut data: [u8; SZ] = [0; SZ];

        self.fill_slot_hash_slice(&mut data, SZ, offset);

        data
    }

    const OPTIMIZE_SMALL_BUF: usize = 0;
}

#[cfg(target_os = "solana")]
struct SlotHashesSysvarProvider {}

#[cfg(target_os = "solana")]
impl SlotHashesProvider for SlotHashesSysvarProvider {
    const OPTIMIZE_SMALL_BUF: usize = 32;

    // copy-paste from solana_program::sysvar::get_sys_var which is declared private for some reason
    fn fill_slot_hash_slice(&self, data: &mut [u8], sz: usize, offset: usize) {
        let sysvar_id = solana_program::slot_hashes::sysvar::id();

        let sysvar_id: *const u8 = std::ptr::from_ref(&sysvar_id).cast::<u8>();

        let var_addr = std::ptr::from_mut(data).cast::<u8>();

        let sz: u64 = sz.try_into().unwrap();
        let offset: u64 = offset.try_into().unwrap();

        let result =
            unsafe { solana_program::syscalls::sol_get_sysvar(sysvar_id, var_addr, offset, sz) };

        assert!(
            result == solana_program::entrypoint::SUCCESS,
            "failed sol_get_sysvar"
        );
    }
}

struct SlotHashesAccountProvider<'a> {
    data: &'a [u8],
}

impl<'a> SlotHashesProvider for SlotHashesAccountProvider<'a> {
    fn fill_slot_hash_slice(&self, data: &mut [u8], sz: usize, offset: usize) {
        data.clone_from_slice(self.data[offset..][..sz].try_into().unwrap());
    }
}

fn find_slot_hash_impl<Provider: SlotHashesProvider>(value: Slot, provider: &Provider) -> [u8; 32] {
    struct SmallHashBuf {
        data: Vec<u8>,
        offset: usize,
    }

    const ONE_SIZE: usize = std::mem::size_of::<PodSlotHash>();

    let slot_hashes_len = u64::from_le_bytes(provider.get_slot_hash_slice::<8>(0));

    // copy-paste from slice::binary_search
    let mut size = usize::try_from(slot_hashes_len).unwrap() - 1;
    let mut left = 0;
    let mut right = size;

    let mut small_buf: Option<SmallHashBuf> = None;

    let to_offset = |x| x * ONE_SIZE + 8; // +8 - the first 8 bytes for the len of vector

    while left < right {
        let mid = left + size / 2;
        let offset = to_offset(mid);

        if size < Provider::OPTIMIZE_SMALL_BUF {
            let mut buf = vec![0_u8; Provider::OPTIMIZE_SMALL_BUF * ONE_SIZE];
            provider.fill_slot_hash_slice(
                buf.borrow_mut(),
                std::cmp::min(Provider::OPTIMIZE_SMALL_BUF, size + 1) * ONE_SIZE,
                to_offset(left),
            );
            small_buf = Some(SmallHashBuf {
                offset: to_offset(left),
                data: buf,
            });
        }

        let slot = small_buf.as_ref().map_or_else(
            || u64::from_le_bytes(provider.get_slot_hash_slice::<8>(offset)),
            |buf| u64::from_le_bytes(buf.data[(offset - buf.offset)..][..8].try_into().unwrap()),
        );
        let cmp = value.cmp(&slot);

        // The reason why we use if/else control flow rather than match
        // is because match reorders comparison operations, which is perf sensitive.
        // This is x86 asm for u8: https://rust.godbolt.org/z/8Y8Pra.
        if cmp == Ordering::Less {
            left = mid + 1;
        } else if cmp == Ordering::Greater {
            right = mid;
        } else {
            return small_buf.as_ref().map_or_else(
                || provider.get_slot_hash_slice::<32>(offset + 8),
                |buf| {
                    buf.data[(offset + 8 - buf.offset)..][..32]
                        .try_into()
                        .unwrap()
                },
            );
        }

        size = right - left;
    }

    generate_fake_slot_hash(value)
}

#[must_use]
pub fn find_slot_hash_provided(value: Slot, slot_hash_data: &[u8]) -> [u8; 32] {
    let provider = SlotHashesAccountProvider {
        data: slot_hash_data,
    };
    find_slot_hash_impl::<SlotHashesAccountProvider>(value, &provider)
}

#[must_use]
#[cfg(target_os = "solana")]
pub fn find_slot_hash(value: Slot) -> [u8; 32] {
    let provider = SlotHashesSysvarProvider {};
    find_slot_hash_impl::<SlotHashesSysvarProvider>(value, &provider)
}

#[must_use]
#[cfg(not(target_os = "solana"))]
pub fn find_slot_hash(value: Slot) -> [u8; 32] {
    generate_fake_slot_hash(value)
}

#[must_use]
pub fn generate_fake_slot_hash(slot: Slot) -> [u8; 32] {
    let slot_bytes: [u8; 8] = slot.to_be_bytes();
    let mut initial = 0;
    for b in slot_bytes {
        if b != 0 {
            break;
        }
        initial += 1;
    }
    let slot_slice = &slot_bytes[initial..];
    let slot_slice_len = slot_slice.len();
    let mut hash = [255; 32];
    hash[32 - slot_slice_len - 1] = 0;
    hash[(32 - slot_slice_len)..].copy_from_slice(slot_slice);
    hash
}

#[test]
fn test_generate_fake_slot_hash() {
    let slot = 0x46;
    let mut expected: [u8; 32] = [255; 32];
    expected[30] = 0;
    expected[31] = 0x46;
    assert_eq!(generate_fake_slot_hash(slot), expected);

    let slot = 0x3e8;
    let mut expected: [u8; 32] = [255; 32];
    expected[29] = 0;
    expected[30] = 0x03;
    expected[31] = 0xe8;
    assert_eq!(generate_fake_slot_hash(slot), expected);
}
