use crate::utils::helpers::{
    is_bloom_authorized, prologue_padding, BLOOM_ADD_PERMISSION, BLOOM_CLOSE_PERMISSION,
    BLOOM_REMOVE_PERMISSION,
};
use borsh::BorshDeserialize;

use crate::{
    account_info::{validate_bloom_filter_account, validate_system_program, AccountInfo},
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::Pubkey,
    state::bloom::CountingBloomFilter,
    utils::cpi::close_program_account,
};

const BLOOM_COUNTER_BYTES: usize = 0xaf898;
const BLOOM_COUNTER_COUNT: u32 = 0x15f130;
const BLOOM_HASH_COUNT: u8 = 10;

/// Views the packed counter region of a Bloom account.
fn bloom_filter(data: &mut [u8]) -> Result<CountingBloomFilter<'_>, ProgramError> {
    if data.len() < BLOOM_COUNTER_BYTES {
        return Err(ProgramError::AccountDataTooSmall);
    }
    Ok(CountingBloomFilter {
        counters: &mut data[..BLOOM_COUNTER_BYTES],
        counter_count: BLOOM_COUNTER_COUNT,
        hash_count: BLOOM_HASH_COUNT,
    })
}

/// Adds a key to the counting Bloom filter after checking the append permission.
pub fn add_bloom_entry(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(6, 4);
    if accounts.len() < 3 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if !is_bloom_authorized::<BLOOM_ADD_PERMISSION>(accounts) {
        return Err(ProgramError::IncorrectAuthority);
    }
    let filter_account = &accounts[1];
    let system_program = &accounts[2];
    validate_system_program(system_program)?;
    validate_bloom_filter_account(filter_account)?;

    let key = <Pubkey as BorshDeserialize>::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let mut data = filter_account.try_borrow_mut_data()?;
    let mut filter = bloom_filter(&mut data)?;
    if filter.contains_bloom_key(&key)? {
        return Err(GoonFiError::BloomKeyAlreadyPresent.into());
    }
    filter.insert_bloom_key(&key)
}

/// Removes a key from the counting Bloom filter after checking the remove permission.
pub fn remove_bloom_entry(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(4, 2);
    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if !is_bloom_authorized::<BLOOM_REMOVE_PERMISSION>(accounts) {
        return Err(ProgramError::IncorrectAuthority);
    }
    let filter_account = &accounts[1];
    let system_program = &accounts[2];
    validate_system_program(system_program)?;
    validate_bloom_filter_account(filter_account)?;

    let key = <Pubkey as BorshDeserialize>::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let mut data = filter_account.try_borrow_mut_data()?;
    let mut filter = bloom_filter(&mut data)?;
    if !filter.contains_bloom_key(&key)? {
        return Err(GoonFiError::BloomKeyNotPresent.into());
    }
    filter.remove_bloom_key(&key)
}

/// Closes the shared Bloom account and returns its lamports to the signer.
pub fn close_bloom_filter(accounts: &[AccountInfo]) -> ProgramResult {
    prologue_padding!(6, 3);
    let [signer, filter, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !is_bloom_authorized::<BLOOM_CLOSE_PERMISSION>(accounts) {
        return Err(ProgramError::IncorrectAuthority);
    }
    validate_bloom_filter_account(filter)?;
    // SAFETY: validation retains no reference to `filter.owner()` across this call.
    unsafe { close_program_account(filter, signer) }
}
