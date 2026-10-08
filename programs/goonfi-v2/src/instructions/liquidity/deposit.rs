use crate::{
    account_info::{validate_token_program, AccountInfo},
    internal::{ProgramError, ProgramResult},
    pubkey,
    state::{
        market_v2::MarketConfigurationState,
        whitelisted_key_registry::{Whitelist, WHITELIST_REGISTRY_ID},
    },
};
use crate::utils::helpers::prologue_padding;
use borsh::BorshDeserialize;
use goonfi_account_validation::{validate_system_program, validate_token_account};

const DEPOSIT_PERMISSION: u64 = 2;

#[inline(always)]
fn validate_deposit_authority(
    accounts: &[AccountInfo],
    authority_key: &pubkey::Pubkey,
) -> ProgramResult {
    if authority_key == &super::LIQUIDITY_AUTHORITY {
        return Ok(());
    }
    let whitelist = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .and_then(|registry| Whitelist::borrow_account(registry).ok())
        .ok_or(ProgramError::IncorrectAuthority)?;
    let allowed = whitelist.active_programs().iter().any(|program| {
        &program.key == authority_key && program.permissions & DEPOSIT_PERMISSION != 0
    });
    if allowed {
        Ok(())
    } else {
        Err(ProgramError::IncorrectAuthority)
    }
}

/// Deposits both market tokens from the signer into the market vaults.
pub fn deposit(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(5, 4);
    if accounts.len() < 11 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let required_accounts = accounts.first_chunk::<11>().unwrap();
    let [
        authority,
        market,
        bid_source,
        ask_source,
        bid_vault,
        ask_vault,
        bid_mint_account,
        ask_mint_account,
        bid_token_program,
        ask_token_program,
        system_program,
    ] = required_accounts;
    if !authority.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    let authority_key = authority.key();
    validate_deposit_authority(accounts, authority_key)?;

    // Validate the ask program only after the bid program passes.
    let bid_program_validation = validate_token_program(bid_token_program);
    if bid_program_validation.is_ok() {
        validate_token_program(ask_token_program)?;
    }
    bid_program_validation?;
    validate_system_program(system_program)?;

    let (bid_amount, ask_amount) = <(u64, u64)>::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let market_state = MarketConfigurationState::borrow_account(market)?;

    let market_key = market.key();
    if bid_vault.key() != market_state.bid_vault_key() {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        bid_vault,
        market_state.bid_mint_key(),
        market_key,
        bid_token_program.key(),
    )?;
    if ask_vault.key() != market_state.ask_vault_key() {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        ask_vault,
        market_state.ask_mint_key(),
        market_key,
        ask_token_program.key(),
    )?;
    validate_token_account(
        bid_source,
        market_state.bid_mint_key(),
        authority_key,
        bid_token_program.key(),
    )?;
    validate_token_account(
        ask_source,
        market_state.ask_mint_key(),
        authority_key,
        ask_token_program.key(),
    )?;

    if bid_token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
        let decimals = market_state.bid_decimals();
        pinocchio_token_2022::instructions::TransferChecked {
            from: bid_source,
            mint: bid_mint_account,
            to: bid_vault,
            authority,
            amount: bid_amount,
            decimals,
            token_program: bid_token_program.key(),
        }
        .invoke()?;
    } else {
        pinocchio_token_2022::instructions::Transfer {
            from: bid_source,
            to: bid_vault,
            authority,
            amount: bid_amount,
            token_program: bid_token_program.key(),
        }
        .invoke()?;
    }

    if ask_token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
        let decimals = market_state.ask_decimals();
        pinocchio_token_2022::instructions::TransferChecked {
            from: ask_source,
            mint: ask_mint_account,
            to: ask_vault,
            authority,
            amount: ask_amount,
            decimals,
            token_program: ask_token_program.key(),
        }
        .invoke()?;
    } else {
        pinocchio_token_2022::instructions::Transfer {
            from: ask_source,
            to: ask_vault,
            authority,
            amount: ask_amount,
            token_program: ask_token_program.key(),
        }
        .invoke()?;
    }
    Ok(())
}
