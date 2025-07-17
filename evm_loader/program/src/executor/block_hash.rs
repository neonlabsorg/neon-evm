use maybe_async::maybe_async;
use solana_program::slot_history::Slot;
use solana_program::sysvar::slot_hashes::{PodSlotHash, SlotHashes};

use std::cmp::Ordering;

use crate::error::Result;
use crate::platform::Platform;

#[maybe_async(?Send)]
trait SlotHashProvider<'a> {
    async fn fill(&self, offset: usize, buffer: &mut [u8]) -> Result<()>;

    async fn get<const L: usize>(&self, offset: usize) -> Result<[u8; L]> {
        let mut buffer = [0_u8; L];
        self.fill(offset, &mut buffer).await?;
        Ok(buffer)
    }

    async fn get_u64(&self, offset: usize) -> Result<u64> {
        let buffer = self.get::<8>(offset).await?;
        Ok(u64::from_le_bytes(buffer))
    }
}

#[maybe_async(?Send)]
impl<'a, T: Platform<'a>> SlotHashProvider<'a> for T {
    async fn fill(&self, offset: usize, buffer: &mut [u8]) -> Result<()> {
        self.get_sysvar_part::<SlotHashes>(offset, buffer).await
    }
}

#[maybe_async(?Send)]
pub async fn find_slot_hash<'a>(value: Slot, platform: &impl Platform<'a>) -> Result<[u8; 32]> {
    struct SmallHashBuf {
        data: Vec<u8>,
        offset: usize,
    }

    const ONE_SIZE: usize = std::mem::size_of::<PodSlotHash>();
    const OPTIMIZE_SMALL_BUF: usize = 32;

    let slot_hashes_len = platform.get_u64(0).await?;

    // copy-paste from slice::binary_search
    let mut size = usize::try_from(slot_hashes_len)? - 1;
    let mut left = 0;
    let mut right = size;

    let mut small_buf: Option<SmallHashBuf> = None;

    let to_offset = |x| x * ONE_SIZE + 8; // +8 - the first 8 bytes for the len of vector

    while left < right {
        let mid = left + size / 2;
        let offset = to_offset(mid);

        if size < OPTIMIZE_SMALL_BUF && small_buf.is_none() {
            let mut buf = vec![0_u8; OPTIMIZE_SMALL_BUF * ONE_SIZE];

            let len = std::cmp::min(OPTIMIZE_SMALL_BUF, size + 1) * ONE_SIZE;
            platform.fill(to_offset(left), &mut buf[..len]).await?;

            small_buf = Some(SmallHashBuf {
                offset: to_offset(left),
                data: buf,
            });
        }

        let slot = match small_buf {
            Some(ref buf) => u64::from_le_bytes(buf.data[(offset - buf.offset)..][..8].try_into()?),
            None => platform.get_u64(offset).await?,
        };

        match value.cmp(&slot) {
            Ordering::Less => left = mid + 1,
            Ordering::Greater => right = mid,
            Ordering::Equal => match small_buf {
                Some(ref buf) => {
                    let slothash = &buf.data[(offset + 8 - buf.offset)..][..32];
                    return Ok(slothash.try_into()?);
                }
                None => {
                    return platform.get::<32>(offset + 8).await;
                }
            },
        }

        size = right - left;
    }

    Ok(generate_fake_slot_hash(value))
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
