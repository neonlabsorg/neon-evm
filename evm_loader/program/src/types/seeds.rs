#![allow(clippy::needless_range_loop)]

use std::{mem::MaybeUninit, ops::Deref};

use allocator_api2::alloc::Allocator;
use solana_program::pubkey::MAX_SEED_LEN;

use crate::types::Vector;

const MAX_SEEDS: usize = 8; // Solana limit is 16, but we don't use that much in practice

pub struct Seed {
    data: [MaybeUninit<u8>; MAX_SEED_LEN],
    len: usize,
}

impl Seed {
    const UNINIT: MaybeUninit<Self> = MaybeUninit::uninit();

    #[must_use]
    pub fn new(seed: &[u8]) -> Self {
        const UNINIT_BYTE: MaybeUninit<u8> = MaybeUninit::uninit();

        let mut data = [UNINIT_BYTE; MAX_SEED_LEN];

        let ptr = data[..seed.len()].as_mut_ptr().cast::<u8>();
        unsafe { ptr.copy_from_nonoverlapping(seed.as_ptr(), seed.len()) };

        Seed {
            data,
            len: seed.len(),
        }
    }
}

impl Deref for Seed {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let ptr: *const u8 = self.data.as_ptr().cast();
        unsafe { std::slice::from_raw_parts(ptr, self.len) }
    }
}

pub struct Seeds {
    data: [MaybeUninit<Seed>; MAX_SEEDS],
    len: usize,
}

impl Seeds {
    #[must_use]
    pub fn new(seeds: &[&[u8]]) -> Self {
        let mut data = [Seed::UNINIT; MAX_SEEDS];

        for (i, seed) in seeds.iter().enumerate() {
            data[i].write(Seed::new(seed));
        }

        Seeds {
            data,
            len: seeds.len(),
        }
    }
}

pub struct SeedsRef<'a> {
    data: [MaybeUninit<&'a [u8]>; MAX_SEEDS],
    len: usize,
}

impl<'a> SeedsRef<'a> {
    #[must_use]
    pub fn new(seeds: &'a Seeds) -> Self {
        const UNINIT: MaybeUninit<&[u8]> = MaybeUninit::uninit();

        let mut data = [UNINIT; MAX_SEEDS];

        for i in 0..seeds.len {
            let seed: &Seed = unsafe { seeds.data[i].assume_init_ref() };
            data[i].write(seed);
        }

        SeedsRef {
            data,
            len: seeds.len,
        }
    }

    #[must_use]
    pub fn as_slices(&self) -> &[&'a [u8]] {
        let ptr: *const &'a [u8] = self.data.as_ptr().cast();
        unsafe { std::slice::from_raw_parts(ptr, self.len) }
    }
}

impl<'a> Deref for SeedsRef<'a> {
    type Target = [&'a [u8]];

    fn deref(&self) -> &Self::Target {
        self.as_slices()
    }
}

pub struct InvokeSeeds<A: Allocator> {
    pub data: Vector<Seeds, A>,
}

impl<A: Allocator> InvokeSeeds<A> {
    #[must_use]
    pub fn new(invoke_seeds: &[&[&[u8]]], allocator: A) -> Self {
        let mut data = Vector::with_capacity_in(invoke_seeds.len(), allocator);

        for seeds in invoke_seeds {
            let seeds = Seeds::new(seeds);
            data.push(seeds);
        }

        Self { data }
    }
}
