pub use crate::utils::helpers::read_oracle_prefix;
pub use pinocchio::account_info::{AccountInfo, Ref as AccountBorrow};

pub use crate::utils::helpers::validate_bloom_filter_account;

/// Accepts either SPL Token program supported by GoonFi.
pub use goonfi_account_validation::validate_token_program;

/// Verifies that an instruction's system-program account has the expected address.
pub use goonfi_account_validation::validate_system_program;

/// Verifies the address of the instructions sysvar account.
pub use goonfi_account_validation::validate_instructions_sysvar;
