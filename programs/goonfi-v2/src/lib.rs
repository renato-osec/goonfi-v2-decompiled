use pinocchio::{
    account_info::AccountInfo, default_panic_handler, program_entrypoint,
    program_error::ProgramError,
};

/// Checks the program address and dispatches a recognized instruction.
fn process_instruction(
    program_id: &pubkey::Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> internal::ProgramResult {
    if program_id.as_slice() != pubkey::ID.as_slice() {
        return Err(ProgramError::IncorrectProgramId);
    }
    match instruction_data.split_first() {
        Some((&tag @ 0..=18, data)) => instructions::dispatch_instruction(tag, accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

pub mod instructions;
pub mod state;
pub mod account_info;
pub mod utils;
program_entrypoint!(process_instruction, 254);

pub mod pubkey;

pub mod logger;

pub mod internal;
pub use internal::GoonFiError;
default_panic_handler!();

pub mod extension;

extern crate pinocchio;
extern crate borsh;
extern crate spl_token_2022;
