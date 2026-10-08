//! Token state aliases and Token-2022 account sizing.

pub use pinocchio_token::{
    instructions::CloseAccount,
    state::{Mint, TokenAccount},
};

pub(crate) fn token2022_account_len(data: &[u8]) -> Result<usize, crate::internal::ProgramError> {
    use crate::internal::ProgramError;
    use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions};
    use spl_token_2022::state::{Account, Mint as Token2022Mint};

    let mint = StateWithExtensions::<Token2022Mint>::unpack(data)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let mint_extensions = mint
        .get_extension_types()
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let required_extensions =
        ExtensionType::get_required_init_account_extensions(&mint_extensions);
    ExtensionType::try_calculate_account_len::<Account>(&required_extensions)
        .map_err(|_| ProgramError::InvalidAccountData)
}

pub use crate::utils::helpers::token_account_space;

pub use goonfi_account_validation::{read_token_account_identity, TokenAccountIdentity};
