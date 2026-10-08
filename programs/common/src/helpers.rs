use pinocchio::{
    account_info::AccountInfo,
    pubkey::Pubkey,
    program_error::ProgramError,
    sysvars::instructions::IntrospectedInstruction,
};

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum RouterInstruction {
    JupiterRoute,
    JupiterSharedRoute,
    JupiterRouteV2,
    JupiterSharedRouteV2,
    OkxDexV2,
    Titan,
    TitanV2,
    Pro,
    DflowAggregatorV4,
    BinanceWalletRouter,
    BinanceWalletVault,
    SecondaryRouter,
    Settler,
    Sun,
}

pub fn account_count(instruction: &IntrospectedInstruction<'_>) -> usize {
    // SAFETY: Pinocchio's borrowed instruction view starts with a two-byte account count.
    usize::from(u16::from_le_bytes(unsafe { *instruction.raw.cast::<[u8; 2]>() }))
}

/// Finds the greatest threshold without consuming the input iterator.
pub fn maximum_threshold<'a, I>(thresholds: &I, initial: u64) -> u64
where
    I: Clone + Iterator<Item = &'a u64>,
{
    thresholds.clone().copied().fold(initial, core::cmp::max)
}

pub trait SwapArgumentReader: Sized {
    type Error;

    fn deserialize(input: &mut &[u8]) -> Result<Self, Self::Error>;
    fn deserialize_flag(input: &mut &[u8]) -> Result<bool, Self::Error>;
}

/// Decodes swap parameters and an optional trailing flag.
pub fn decode_swap_arguments<T: SwapArgumentReader>(
    mut input: &[u8],
) -> Result<(T, bool), ProgramError> {
    let arguments = T::deserialize(&mut input)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let trailing_flag = if input.is_empty() {
        false
    } else {
        T::deserialize_flag(&mut input)
            .map_err(|_| ProgramError::InvalidInstructionData)?
    };
    Ok((arguments, trailing_flag))
}

/// Get the token account amount for either supported token program.
#[inline(always)]
pub fn get_token_account_amount(
    token_account: &AccountInfo,
    token_program: &Pubkey,
) -> Result<u64, ProgramError> {
    if token_program.eq(&pinocchio_token::ID) {
        Ok(
            pinocchio_token::state::TokenAccount::from_account_info(token_account)
                .map_err(|_| ProgramError::InvalidAccountData)?
                .amount(),
        )
    } else if token_program.eq(&pinocchio_token_2022::ID) {
        Ok(
            pinocchio_token_2022::state::TokenAccount::from_account_info(token_account)
                .map_err(|_| ProgramError::InvalidAccountData)?
                .amount(),
        )
    } else {
        Err(ProgramError::IncorrectProgramId)
    }
}

#[inline]
pub fn route_result_ok(value: bool) -> Result<bool, ProgramError> {
    // Reapply the input flag twice to the accumulated result.
    let mut result = value;
    for _ in 0..2 {
        result &= value;
    }
    Ok(result)
}
