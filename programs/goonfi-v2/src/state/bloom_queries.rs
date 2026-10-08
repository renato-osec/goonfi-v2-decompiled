use crate::internal::ProgramError;
use crate::state::bloom::{
    contains_bloom_key_in_view, CountingBloomView,
};

/// Tests every counter selected by the key's Keccak digest.
pub fn contains_bloom_key_in_data(
    filter: &CountingBloomView<'_>,
    value: &[u8],
) -> Result<bool, ProgramError> {
    contains_bloom_key_in_view(filter, value)
}

pub use crate::utils::helpers::bloom_account_contains_key;
