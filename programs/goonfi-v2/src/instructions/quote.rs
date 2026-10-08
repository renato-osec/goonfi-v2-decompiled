use crate::utils::helpers::prologue_padding;
use crate::{
    account_info::{read_oracle_prefix, AccountInfo},
    internal::{sysvars::get_clock, ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
    state::market_v2::{MarketConfigurationState, MarketPricingView, PricingClass},
    utils::cpi::{classify_quote_route, decode_quote_route, RouteClass},
};
use borsh::BorshDeserialize;
use goonfi_account_validation::validate_instructions_sysvar;

#[derive(BorshDeserialize)]
struct SimulateOutAmount {
    amount_in: u64,
    minimum_amount_out: u64,
}

fn decode_quote_request(mut input: &[u8]) -> Option<(SimulateOutAmount, bool)> {
    let request = SimulateOutAmount::deserialize(&mut input).ok()?;
    let is_fee_payer = if input.is_empty() {
        false
    } else {
        bool::deserialize(&mut input).ok()?
    };
    Some((request, is_fee_payer))
}

fn exceeds_price_threshold(
    market: &MarketConfigurationState,
    current_price: u64,
    last_traded_price: u64,
    is_user_bid: bool,
) -> bool {
    let tolerance = market.reserve_tolerance();
    // Only adverse moves from the last traded price count against the tolerance.
    let deviation = if last_traded_price != 0 && tolerance != 0 {
        if is_user_bid {
            if current_price < last_traded_price {
                Some(last_traded_price.saturating_sub(current_price))
            } else {
                None
            }
        } else if current_price > last_traded_price {
            Some(current_price.saturating_sub(last_traded_price))
        } else {
            None
        }
    } else {
        None
    };
    deviation.is_some_and(|deviation| {
        let observed = u128::from(deviation) * 1_000_000;
        let allowed = u128::from(tolerance) * u128::from(last_traded_price);
        observed > allowed
    })
}

/// Publishes a quote; invalid requests produce a zero quote.
pub fn quote_swap(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(6, 3);
    let out_amount = compute_out_amount(accounts, instruction_data).unwrap_or_default();
    pinocchio::cpi::set_return_data(&out_amount.to_le_bytes());
    Ok(())
}

fn compute_out_amount(
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<u64, ProgramError> {
    let [market_account, oracle_account, vault_input, vault_output, instructions_account, ..] =
        accounts
    else {
        return Err(ProgramError::InvalidAccountData);
    };
    validate_instructions_sysvar(instructions_account)?;

    let market_data = MarketConfigurationState::borrow_account(market_account)?;
    let market: &MarketConfigurationState = &market_data;
    if oracle_account.key() != market.oracle_key() || market.quotes_disabled() {
        return Err(ProgramError::InvalidAccountData);
    }

    let (request, is_fee_payer) =
        decode_quote_request(instruction_data).ok_or(ProgramError::InvalidInstructionData)?;
    if request.amount_in == 0 {
        return Err(ProgramError::InvalidAccountData);
    }

    let is_user_bid = if vault_input.key() == market.ask_vault_key()
        && vault_output.key() == market.bid_vault_key()
    {
        true
    } else if vault_input.key() == market.bid_vault_key()
        && vault_output.key() == market.ask_vault_key()
    {
        false
    } else {
        return Err(ProgramError::InvalidAccountData);
    };
    let route = decode_quote_route(instructions_account)?;
    let route_class_hint = Some(if matches!(route.classification, RouteClass::Titan) {
        PricingClass::TitanQuote
    } else {
        PricingClass::StandardQuote
    });
    let route_class = classify_quote_route(
        &Pubkey::default(),
        is_fee_payer,
        route_class_hint,
        2,
        &route,
        instructions_account,
        market.bid_mint_key(),
        market.ask_mint_key(),
    )?;

    let context = read_oracle_prefix(oracle_account)?;
    let baseline_ref = if is_user_bid {
        market.ask_reserve_baseline_ref()
    } else {
        market.bid_reserve_baseline_ref()
    };
    let baseline = u64::from_le(*baseline_ref);
    let current_price = if is_user_bid {
        context.ask_price
    } else {
        context.bid_price
    };
    if exceeds_price_threshold(market, current_price, baseline, is_user_bid) {
        return Err(ProgramError::InvalidAccountData);
    }

    let token_account = if is_user_bid {
        vault_output
    } else {
        vault_input
    };
    let token_program = market.bid_token_program();
    let vault_amount = if token_program == &pubkey::TOKEN_PROGRAM_ID {
        let state = pinocchio_token::state::TokenAccount::from_account_info(token_account)?;
        state.amount()
    } else if token_program == &pubkey::TOKEN_2022_PROGRAM_ID {
        let state = pinocchio_token_2022::state::TokenAccount::from_account_info(token_account)?;
        state.amount()
    } else {
        return Err(ProgramError::InvalidAccountData);
    };
    let clock = get_clock()?;
    let pricing_market = MarketPricingView::from_account_data(bytemuck::bytes_of(market))?;
    let outcome = pricing_market.calculate_quote_price(
        request.amount_in,
        request.minimum_amount_out,
        clock.slot,
        is_user_bid,
        vault_amount,
        route_class,
        context,
    )?;
    Ok(outcome.quote_amount)
}
