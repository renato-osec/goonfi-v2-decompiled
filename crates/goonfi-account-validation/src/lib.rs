#![no_std]

use pinocchio::{
    account_info::AccountInfo, instruction::{Seed, Signer}, program_error::ProgramError,
    pubkey::Pubkey, sysvars::{instructions::INSTRUCTIONS_ID, rent::Rent, Sysvar}, ProgramResult,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};
use pinocchio_token::state::TokenAccount;
use pinocchio_token_2022::state::TokenAccount as Token2022Account;

/// Verifies the system-program address in an instruction's account list.
pub fn validate_system_program(account: &AccountInfo) -> ProgramResult {
    if account.key() == &pinocchio_system::ID {
        Ok(())
    } else {
        Err(ProgramError::IncorrectProgramId)
    }
}

/// Accepts either supported token program.
pub fn validate_token_program(account: &AccountInfo) -> ProgramResult {
    if account.key() != &pinocchio_token::ID
        && account.key() != &pinocchio_token_2022::ID
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// Verifies the address of the instructions sysvar account.
pub fn validate_instructions_sysvar(account: &AccountInfo) -> ProgramResult {
    if account.key() == &INSTRUCTIONS_ID {
        Ok(())
    } else {
        Err(ProgramError::IncorrectProgramId)
    }
}

/// Checks a token account's mint and authority for its token program.
pub fn validate_token_account(
    account: &AccountInfo,
    expected_mint: &Pubkey,
    expected_owner: &Pubkey,
    token_program: &Pubkey,
) -> ProgramResult {
    let (owner, mint) = if token_program == &pinocchio_token::ID {
        let state = TokenAccount::from_account_info(account)
            .map_err(|_| ProgramError::InvalidAccountData)?;
        (*state.owner(), *state.mint())
    } else if token_program == &pinocchio_token_2022::ID {
        let state = Token2022Account::from_account_info(account)
            .map_err(|_| ProgramError::InvalidAccountData)?;
        (*state.owner(), *state.mint())
    } else {
        return Err(ProgramError::IncorrectProgramId);
    };

    if &owner != expected_owner {
        return Err(ProgramError::InvalidAccountOwner);
    }
    if &mint != expected_mint {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(())
}

/// The owner and mint shared by legacy and Token-2022 accounts.
pub struct TokenAccountIdentity {
    pub owner: Pubkey,
    pub mint: Pubkey,
}

/// Reads the identity of either supported token account.
pub fn read_token_account_identity(
    account: &AccountInfo,
    token_program: &Pubkey,
) -> Result<TokenAccountIdentity, ProgramError> {
    if token_program == &pinocchio_token::ID {
        let state = TokenAccount::from_account_info(account)
            .map_err(|_| ProgramError::InvalidAccountData)?;
        return Ok(TokenAccountIdentity {
            owner: *state.owner(),
            mint: *state.mint(),
        });
    }
    if token_program != &pinocchio_token_2022::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let state = Token2022Account::from_account_info(account)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(TokenAccountIdentity {
        owner: *state.owner(),
        mint: *state.mint(),
    })
}

mod route_classifier;

/// Funds and creates a program account, or prepares an already funded account.
pub fn create_or_allocate_account(
    account: &AccountInfo,
    owner: &Pubkey,
    space: u64,
    seeds: &[Seed],
    payer: &AccountInfo,
    rent_override: Option<Rent>,
) -> ProgramResult {
    let signers = [Signer::from(seeds)];
    let rent = rent_override.unwrap_or(Rent::get()?);
    let current_lamports = *account.try_borrow_lamports()?;
    let lamports = rent.minimum_balance(space as usize);

    if current_lamports == 0 {
        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space,
            owner,
        }
        .invoke_signed(&signers)?;
    } else {
        let required_lamports = lamports.saturating_sub(*account.try_borrow_lamports()?);
        if required_lamports > 0 {
            Transfer {
                from: payer,
                to: account,
                lamports: required_lamports,
            }
            .invoke()?;
        }

        Allocate { account, space }.invoke_signed(&signers)?;
        // Assign receives its own copy of the PDA signer seeds.
        Assign { account, owner }.invoke_signed(&signers.clone())?;
    }

    Ok(())
}

/// Transfers the balance and closes a program-owned account.
///
/// # Safety
/// Callers must not keep a reference from `source.owner()` alive across this call.
// SAFETY: the caller upholds the documented owner-reference requirement.
pub unsafe fn close_program_account(
    source: &AccountInfo,
    destination: &AccountInfo,
) -> ProgramResult {
    let balance = destination
        .lamports()
        .checked_add(source.lamports())
        .ok_or(ProgramError::ArithmeticOverflow)?;

    *destination.try_borrow_mut_lamports()? = balance;

    {
        let mut source_balance = source.try_borrow_mut_lamports()?;
        *source_balance = 0;
    }
    // SAFETY: the caller holds no source owner reference, and the source's
    // mutable lamports borrow has ended before its owner is changed.
    unsafe { source.assign(&pinocchio_system::ID) };
    source.resize(0)
}

mod route_trade;
mod route_decoder;
mod route_pubkey;
mod route_validation;
mod route_introspection;
mod signed_route;
mod quote_route;
/// Route program ID registries shared with the route decoder.
pub mod route_tables;

pub use route_decoder::{decode_quote_route, RouteSummary};
pub use quote_route::classify_quote_route;
pub use route_introspection::route_authority;
pub use route_validation::{route_changes_stablecoin_pair, route_uses_same_mint};
pub use route_trade::validate_route_trade;
pub use signed_route::{
    classify_signed_route, transaction_has_recognized_signer,
};
