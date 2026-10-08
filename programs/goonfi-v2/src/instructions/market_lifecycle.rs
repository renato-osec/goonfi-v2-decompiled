use super::market_admin::{InitializeMarketParameters, MARKET_ADMINISTRATOR};
use crate::state::market_v2::create_market;
use crate::{
    account_info::AccountInfo,
    internal::{ProgramError, ProgramResult},
    state::{
        market_v2::MarketConfigurationState,
        whitelisted_key_registry::{
            Whitelist, MAX_WHITELIST_PROGRAMS, WHITELIST_REGISTRY_ID,
        },
    },
    utils::helpers::prologue_padding,
};

/// Closes both empty market vaults and returns the market account's balance.
pub fn close_market(accounts: &[AccountInfo]) -> ProgramResult {
    prologue_padding!(12, 3);
    let [
        authority,
        market_account,
        bid_vault_account,
        ask_vault_account,
        bid_program,
        ask_program,
        system_program,
        ..
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if check_market_permission::<0x10>(accounts, authority).is_err() {
        return Err(ProgramError::IncorrectAuthority);
    }
    crate::account_info::validate_token_program(bid_program)?;
    crate::account_info::validate_token_program(ask_program)?;
    crate::account_info::validate_system_program(system_program)?;

    {
        let market = MarketConfigurationState::borrow_account(market_account)?;

        if bid_vault_account.key() != market.bid_vault_key() {
            return Err(ProgramError::InvalidAccountData);
        }
        crate::utils::cpi::validate_token_account(
            bid_vault_account,
            market.bid_mint_key(),
            market_account.key(),
            market.bid_token_program(),
        )?;
        if ask_vault_account.key() != market.ask_vault_key() {
            return Err(ProgramError::InvalidAccountData);
        }
        crate::utils::cpi::validate_token_account(
            ask_vault_account,
            market.ask_mint_key(),
            market_account.key(),
            market.ask_token_program(),
        )?;
        market.close_market_vault(
            market_account,
            bid_vault_account,
            authority,
            market.bid_token_program(),
        )?;
        market.close_market_vault(
            market_account,
            ask_vault_account,
            authority,
            market.ask_token_program(),
        )?;
    }
    // SAFETY: the market borrow ended above; no owner reference remains live.
    unsafe { crate::utils::cpi::close_program_account(market_account, authority) }?;
    Ok(())
}

/// Validates the accounts and decodes the parameters for market creation.
pub fn initialize_market(accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    prologue_padding!(9, 2);
    let [payer, market, bid_mint, ask_mint, bid_vault, ask_vault, oracle, bid_program, ask_program, system_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if check_market_permission::<0x1>(accounts, payer).is_err() {
        return Err(ProgramError::IncorrectAuthority);
    }
    crate::account_info::validate_token_program(bid_program)?;
    crate::account_info::validate_token_program(ask_program)?;
    crate::account_info::validate_system_program(system_program)?;
    let parameters = std::boxed::Box::new(
        <InitializeMarketParameters as borsh::BorshDeserialize>::try_from_slice(input)
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );
    create_market(
        payer,
        market,
        bid_mint,
        ask_mint,
        bid_vault,
        ask_vault,
        oracle,
        bid_program,
        ask_program,
        parameters,
    )?;
    Ok(())
}

fn check_market_permission<const PERMISSION: u64>(
    accounts: &[AccountInfo],
    authority: &AccountInfo,
) -> ProgramResult {
    if !authority.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if authority.key() == &MARKET_ADMINISTRATOR {
        return Ok(());
    }
    let registry = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .ok_or(ProgramError::IncorrectAuthority)?;
    let state = Whitelist::borrow_account(registry).ok();
    let denied = state.is_none_or(|state| {
        let count = (state.program_count() as usize).min(MAX_WHITELIST_PROGRAMS);
        state.programs()[..count]
            .iter()
            .find(|entry| {
                &entry.key == authority.key() && entry.permissions & PERMISSION != 0
            })
            .is_none()
    });
    if denied {
        Err(ProgramError::IncorrectAuthority)
    } else {
        Ok(())
    }
}
