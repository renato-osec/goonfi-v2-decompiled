//! Shared account and instruction helpers.

use crate::{
    account_info::AccountInfo,
    internal::ProgramError,
    pubkey::Pubkey,
    state::{
        bloom::{
            CountingBloomView, DEFAULT_BLOOM_COUNTER_COUNT, DEFAULT_BLOOM_HASH_COUNT,
        },
        whitelisted_key_registry::{Whitelist, WHITELIST_REGISTRY_ID},
    },
};
use oracle_asm::Oracle;

/// Performs volatile writes to a local `u32` before instruction work.
/// Values start at `first` and increase by `step` until a value above 100
/// has been written; the slot is never read.
macro_rules! prologue_padding {
    ($first:expr, $step:expr) => {{
        let mut slot = 0u32;
        let mut value: u32 = $first;
        loop {
            // SAFETY: `slot` is a live, properly aligned local `u32`.
            unsafe { core::ptr::write_volatile(&mut slot, value) };
            if value > 100 {
                break;
            }
            value += $step;
        }
    }};
}
pub(crate) use prologue_padding;

const BLOOM_AUTHORITY: Pubkey = [
    0x0d, 0x88, 0x0e, 0xee, 0x7b, 0xa8, 0x04, 0x97, 0x27, 0x12, 0xaf, 0x88, 0x3a, 0x68, 0x7c, 0x6e,
    0xbb, 0x50, 0xd3, 0xe8, 0x54, 0x28, 0x2b, 0x87, 0xac, 0xca, 0x77, 0xfa, 0x01, 0xde, 0xd9, 0xba,
];
pub(crate) const BLOOM_ADD_PERMISSION: u64 = 0x40;
pub(crate) const BLOOM_REMOVE_PERMISSION: u64 = 0x80;
pub(crate) const BLOOM_CLOSE_PERMISSION: u64 = 0x100;

/// A signed primary authority or a registry program with the requested permission may operate
/// on the shared Bloom account.
#[inline(always)]
pub(crate) fn is_bloom_authorized<const PERMISSION: u64>(accounts: &[AccountInfo]) -> bool {
    let signer = &accounts[0];
    if !signer.is_signer() {
        return false;
    }
    let signer_key = signer.key();
    if signer_key == &BLOOM_AUTHORITY {
        return true;
    }

    let Some(registry) = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
    else {
        return false;
    };
    if let Ok(whitelist) = Whitelist::borrow_account(registry) {
        return whitelist.active_programs().iter().any(|program| {
            &program.key == signer_key && program.permissions & PERMISSION != 0
        });
    }
    false
}

/// Queries a public key against a Bloom account's packed counters.
pub fn bloom_account_contains_key(
    account: &AccountInfo,
    key: &Pubkey,
) -> Result<bool, ProgramError> {
    let data = account.try_borrow_data()?;
    let filter = CountingBloomView::from_account_data(
        &data,
        DEFAULT_BLOOM_COUNTER_COUNT,
        DEFAULT_BLOOM_HASH_COUNT,
    )?;
    crate::instructions::bloom_queries::contains_bloom_key_in_data(&filter, key)
}

/// Validates the registry's Bloom-filter account before it is used.
pub fn validate_bloom_filter_account(account: &AccountInfo) -> crate::internal::ProgramResult {
    use crate::pubkey::{BLOOM_FILTER_ACCOUNT, ID};
    if account.key() != &BLOOM_FILTER_ACCOUNT {
        return Err(ProgramError::InvalidAccountData);
    }
    if !account.is_owned_by(&ID) {
        return Err(ProgramError::IncorrectProgramId);
    }
    if account.lamports() == 0 {
        return Err(ProgramError::UninitializedAccount);
    }
    let data_len = account.try_borrow_data()?.len();
    if data_len < (DEFAULT_BLOOM_COUNTER_COUNT as usize).div_ceil(2) {
        Err(ProgramError::AccountDataTooSmall)
    } else {
        Ok(())
    }
}

/// Decodes an owned oracle account's 32-byte price prefix.
pub fn read_oracle_prefix(
    account: &AccountInfo,
) -> Result<Oracle, ProgramError> {
    if account.data_len() < Oracle::SIZE {
        return Err(ProgramError::InvalidAccountData);
    }
    if account.owner() != &oracle_asm::ID {
        return Err(ProgramError::InvalidAccountOwner);
    }
    let data = account.try_borrow_data()?;
    let oracle_bytes: [u8; Oracle::SIZE] = data[..Oracle::SIZE]
        .try_into()
        .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(Oracle::from_bytes(&oracle_bytes))
}

/// Returns the space needed for a token account belonging to this mint.
pub fn token_account_space(
    mint_account: &AccountInfo,
    token_program: &Pubkey,
) -> Result<usize, ProgramError> {
    use crate::pubkey::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};

    if token_program.as_slice() == TOKEN_PROGRAM_ID.as_slice() {
        return Ok(crate::state::token::TokenAccount::LEN);
    }
    if token_program.as_slice() == TOKEN_2022_PROGRAM_ID.as_slice() {
        let data = mint_account.try_borrow_data()?;
        crate::state::token::token2022_account_len(&data)
    } else {
        Err(ProgramError::IncorrectProgramId)
    }
}
