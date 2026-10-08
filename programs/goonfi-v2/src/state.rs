/// A counting Bloom filter packs two saturating four-bit counters per byte.
pub mod bloom;
pub mod bloom_queries;

/// Reads one little-endian quantity from a validated serialized field.
fn read_u64<const N: usize>(data: &[u8; N], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

pub mod market_v2;
pub mod whitelisted_key_registry;

/// Fixed portion of the serialized swap parameters.
#[derive(borsh::BorshDeserialize)]
pub struct SwapArguments {
    pub direction: bool,
    pub amount: u64,
    pub other_amount_threshold: u64,
}

impl goonfi_common::helpers::SwapArgumentReader for SwapArguments {
    type Error = std::io::Error;

    #[inline]
    fn deserialize(input: &mut &[u8]) -> Result<Self, Self::Error> {
        <Self as borsh::BorshDeserialize>::deserialize(input)
    }

    fn deserialize_flag(input: &mut &[u8]) -> Result<bool, Self::Error> {
        <bool as borsh::BorshDeserialize>::deserialize(input)
    }
}

/// Decodes fixed swap parameters and an optional trailing flag.
#[inline]
pub fn decode_swap_arguments(
    input: &[u8],
) -> Result<(SwapArguments, bool), crate::internal::ProgramError> {
    goonfi_common::helpers::decode_swap_arguments::<SwapArguments>(input)
}

pub use goonfi_common::helpers::maximum_threshold;

pub mod token;
