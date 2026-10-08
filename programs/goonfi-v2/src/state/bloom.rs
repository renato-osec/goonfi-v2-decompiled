use core::marker::PhantomData;

use crate::internal::{ProgramError, ProgramResult};
use tiny_keccak::{Hasher, Keccak};

/// Borrows packed four-bit counters; insertion and removal mutate them in place.
pub struct CountingBloomFilter<'a> {
    pub counters: &'a mut [u8],
    pub counter_count: u32,
    pub hash_count: u8,
}

/// A read-only Bloom filter backed by account data.
pub struct CountingBloomView<'a> {
    counter_count: u32,
    hash_count: u8,
    counter_bytes: *const u8,
    counter_bytes_len: usize,
    _data: PhantomData<&'a [u8]>,
}

/// Tests every counter selected by the key's Keccak digest.
pub(crate) fn contains_bloom_key_in_view(
    filter: &CountingBloomView<'_>,
    value: &[u8],
) -> Result<bool, ProgramError> {
    let count = u64::from(filter.counter_count);
    // SAFETY: construction checks the length and ties the pointer to the source slice's lifetime.
    let counters = unsafe {
        core::slice::from_raw_parts(filter.counter_bytes, filter.counter_bytes_len)
    };
    let mut remaining = filter.hash_count;
    let (mut probe, step) = CountingBloomFilter::probe_seed(value, count);
    let found = loop {
        if remaining == 0 {
            break true;
        }
        probe %= count;
        let byte = match counters.get((probe >> 1) as usize) {
            Some(byte) => *byte,
            None => return Ok(false),
        };
        let counter = if probe & 1 == 0 { byte & 15 } else { byte >> 4 };
        probe = probe.wrapping_add(step);
        remaining -= 1;
        if counter == 0 {
            break false;
        }
    };
    Ok(found)
}

impl<'a> CountingBloomView<'a> {
    pub fn from_account_data(
        data: &'a [u8],
        counter_count: u32,
        hash_count: u8,
    ) -> Result<Self, ProgramError> {
        let required = validate_params(counter_count, hash_count)?;
        if data.len() < required {
            return Err(ProgramError::AccountDataTooSmall);
        }
        Ok(Self {
            counter_count,
            hash_count,
            counter_bytes: data.as_ptr(),
            counter_bytes_len: required,
            _data: PhantomData,
        })
    }
}

impl CountingBloomFilter<'_> {
    /// Counts down the key's hash probes, saturating each selected counter at fifteen.
    pub fn insert_bloom_key(&mut self, value: &[u8]) -> ProgramResult {
        let count = u64::from(self.counter_count);
        let remaining = self.hash_count;
        let (mut probe, step) = CountingBloomFilter::probe_seed(value, count);
        (0..remaining).rev().try_for_each(move |_| -> Result<(), BloomMutationError> {
            probe %= count;
            let byte = self.counters.get_mut((probe >> 1) as usize)
                .ok_or(BloomMutationError(ProgramError::InvalidAccountData))?;
            let mut counter = *byte & 15;
            if probe & 1 != 0 {
                counter = *byte >> 4;
            }
            let incremented = if counter == 15 { 15 } else { counter + 1 };
            *byte = if probe & 1 == 0 {
                (*byte & 0xf0) | incremented
            } else {
                (*byte & 0x0f) | (incremented << 4)
            };
            probe = probe.wrapping_add(step);
            Ok(())
        }).map_err(|error| error.0)
    }

    /// Counts down the key's hash probes, leaving zero counters unchanged.
    /// Updating one packed counter preserves its neighboring nibble.
    pub fn remove_bloom_key(&mut self, value: &[u8]) -> ProgramResult {
        let count = u64::from(self.counter_count);
        let remaining = self.hash_count;
        let (mut probe, step) = CountingBloomFilter::probe_seed(value, count);
        (0..remaining).rev().try_for_each(move |_| -> Result<(), BloomMutationError> {
            let current_probe = probe % count;
            probe = current_probe.wrapping_add(step);
            let byte = self.counters.get_mut((current_probe >> 1) as usize)
                .ok_or(BloomMutationError(ProgramError::InvalidAccountData))?;
            let original = *byte;
            let updated = if current_probe & 1 == 0 {
                (original & 0xf0) | (original & 0x0f).saturating_sub(1)
            } else {
                (original & 0x0f) | (((original >> 4) & 0x0f).saturating_sub(1) << 4)
            };
            *byte = updated;
            Ok(())
        }).map_err(|error| error.0)
    }

    /// Tests whether every probe has a nonzero counter.
    /// Zero counters rule out membership; nonzero counters may collide.
    pub fn contains_bloom_key(&self, value: &[u8]) -> Result<bool, ProgramError> {
        let count = u64::from(self.counter_count);
        let counters = &self.counters;
        let mut remaining = self.hash_count;
        let (mut probe, step) = CountingBloomFilter::probe_seed(value, count);
        let found = loop {
            if remaining == 0 {
                break true;
            }
            probe %= count;
            let byte = match counters.get((probe >> 1) as usize) {
                Some(byte) => *byte,
                None => return Ok(false),
            };
            let counter = if probe & 1 == 0 { byte & 15 } else { byte >> 4 };
            probe = probe.wrapping_add(step);
            remaining -= 1;
            if counter == 0 {
                break false;
            }
        };
        Ok(found)
    }

    /// Uses two digest words for the starting probe and a nonzero step.
    pub(crate) fn probe_seed(value: &[u8], counter_count: u64) -> (u64, u64) {
        let mut hasher = Keccak::v256();
        hasher.update(value);
        let mut digest = [0u8; 32];
        hasher.finalize(&mut digest);
        let start = u64::from_le_bytes(digest[..8].try_into().unwrap());
        let h2 = u64::from_le_bytes(digest[8..16].try_into().unwrap()) % counter_count;
        (start, h2.max(1))
    }
}

// Carries invalid account data through a fallible probe update.
struct BloomMutationError(ProgramError);

pub const DEFAULT_BLOOM_COUNTER_COUNT: u32 = 1_438_000;
pub const DEFAULT_BLOOM_HASH_COUNT: u8 = 10;

const fn required_bytes(counter_count: u32) -> usize {
    (counter_count as usize).div_ceil(2)
}

fn validate_params(counter_count: u32, hash_count: u8) -> Result<usize, ProgramError> {
    if counter_count == 0 || hash_count == 0 {
        return Err(ProgramError::InvalidArgument);
    }
    Ok(required_bytes(counter_count))
}
