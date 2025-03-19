use solana_program::slot_history::Slot;
use std::{borrow::BorrowMut, cmp::Ordering};
//use solana_program::sysvar::slot_hashes::PodSlotHashes;

#[allow(dead_code)]
fn sol_get_sysvar_stub(
    _sysvar_id_addr: *const u8,
    _result: *mut u8,
    _offset: u64,
    _length: u64,
) -> u64 {
    solana_program::entrypoint::SUCCESS
}

// copy-paste from solana_program::sysvar::get_sys_var which is declared private for some reason
#[allow(clippy::ref_as_ptr)]
#[allow(clippy::ptr_as_ptr)]
fn fill_sysvar_slot_hash_slice(data: &mut [u8], sz: usize, offset: usize) {
    let sysvar_id = solana_program::slot_hashes::sysvar::id();

    let sysvar_id = &sysvar_id as *const _ as *const u8;

    let var_addr = data as *mut _ as *mut u8;

    let sz: u64 = sz.try_into().unwrap();
    let offset: u64 = offset.try_into().unwrap();

    #[cfg(target_os = "solana")]
    let result =
        unsafe { solana_program::syscalls::sol_get_sysvar(sysvar_id, var_addr, offset, sz) };

    #[cfg(not(target_os = "solana"))]
    let result = sol_get_sysvar_stub(sysvar_id, var_addr, offset, sz);

    assert!(
        result == solana_program::entrypoint::SUCCESS,
        "failed sol_get_sysvar"
    );
}

fn get_sysvar_slot_hash_slice<const SZ: usize>(offset: usize) -> [u8; SZ] {
    let mut data: [u8; SZ] = [0; SZ];

    fill_sysvar_slot_hash_slice(&mut data, SZ, offset);

    data
}

#[must_use]
pub fn find_slot_hash(value: Slot, _slot_hashes_data: &[u8]) -> [u8; 32] {
    struct SmallHashBuf {
        data: Vec<u8>,
        offset: usize,
    }

    const SIZE_TRESHOLD: usize = 32;
    const ONE_SIZE: usize = 40;
    const BUF_SIZE: usize = ONE_SIZE * SIZE_TRESHOLD;

    let slot_hashes_len = u64::from_le_bytes(get_sysvar_slot_hash_slice::<8>(0));

    // copy-paste from slice::binary_search
    let mut size = usize::try_from(slot_hashes_len).unwrap() - 1;
    let mut left = 0;
    let mut right = size;

    let mut small_buf: Option<SmallHashBuf> = None;

    let to_offset = |x| x * ONE_SIZE + 8; // +8 - the first 8 bytes for the len of vector

    while left < right {
        let mid = left + size / 2;
        let offset = to_offset(mid);

        if size < SIZE_TRESHOLD {
            let mut buf = vec![0_u8; BUF_SIZE];
            fill_sysvar_slot_hash_slice(buf.borrow_mut(), BUF_SIZE, to_offset(left));
            small_buf = Some(SmallHashBuf {
                offset: to_offset(left),
                data: buf,
            });
        }

        let slot = small_buf.as_ref().map_or_else(
            || u64::from_le_bytes(get_sysvar_slot_hash_slice::<8>(offset)),
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
                || get_sysvar_slot_hash_slice::<32>(offset + 8),
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
