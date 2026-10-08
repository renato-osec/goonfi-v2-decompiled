use crate::account_info::validate_token_program;
use crate::{
    account_info::{
        read_oracle_prefix, validate_bloom_filter_account, validate_instructions_sysvar,
        AccountInfo,
    },
    extension::{Seed, Signer},
    instructions::bloom_queries::bloom_account_contains_key,
    internal::{
        instruction_sysvar::{classify_signed_route, transaction_has_recognized_signer},
        sysvars::get_clock,
        GoonFiError, ProgramError, ProgramResult,
    },
    logger::LogBuffer,
    pubkey::{self, Pubkey, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID},
    state::{
        decode_swap_arguments,
        market_v2::{MarketConfigurationState, MarketPricingView, PriceContext, PricingClass},
    },
    utils::{
        cpi::{classify_quote_route, decode_quote_route, RouteClass},
        helpers::prologue_padding,
    },
};
use goonfi_account_validation::validate_token_account;

const JITO_ROUTE_PROGRAM: Pubkey = [
    0xfc, 0xd1, 0x3d, 0xa0, 0x41, 0x55, 0x90, 0x13, 0xcf, 0x4a, 0xeb, 0xa1, 0xa8, 0x4e, 0xd7, 0x02,
    0x3a, 0xd4, 0xff, 0x21, 0xa8, 0x47, 0x47, 0xff, 0x3b, 0x05, 0xab, 0x30, 0xfa, 0x82, 0x07, 0xc3,
];
const AUXILIARY_ROUTE_AUTHORITY: Pubkey = [
    0x3c, 0xcc, 0xcd, 0xa4, 0xcc, 0x42, 0x3a, 0x23, 0x86, 0xe6, 0xa1, 0xf0, 0x8f, 0x2f, 0xfa, 0x25,
    0x1f, 0x44, 0x00, 0x03, 0xd4, 0x95, 0xda, 0xa1, 0x1a, 0xa8, 0x2f, 0x50, 0x69, 0x12, 0x5a, 0xb2,
];
const PERMITTED_ROUTE_AUTHORITIES: [Pubkey; 5] = [
    pubkey::RECOGNIZED_ROUTE_SIGNERS[3],
    pubkey::RECOGNIZED_ROUTE_SIGNERS[1],
    pubkey::RECOGNIZED_ROUTE_SIGNERS[2],
    [
        0x95, 0x75, 0x38, 0x8a, 0xac, 0xc5, 0x4a, 0xa8, 0x85, 0x85, 0x8a, 0xa0, 0x59, 0x35, 0x46,
        0x90, 0xf9, 0x59, 0x83, 0x32, 0x72, 0x2d, 0x0f, 0x30, 0x21, 0x32, 0x1d, 0x8b, 0xcb, 0x3b,
        0x1f, 0x04,
    ],
    [
        0x2c, 0x15, 0x70, 0x0c, 0x46, 0x9d, 0xa5, 0xae, 0x43, 0x7c, 0xe9, 0x78, 0xea, 0xd6, 0x51,
        0xd2, 0x4f, 0xb6, 0x77, 0x18, 0x07, 0xdd, 0xe4, 0xd8, 0x5b, 0x60, 0x97, 0x48, 0xed, 0x34,
        0x6b, 0xf4,
    ],
];

pub(crate) static COMPUTE_EXEMPT_SIGNERS: [Pubkey; 2] = [
    PERMITTED_ROUTE_AUTHORITIES[3],
    PERMITTED_ROUTE_AUTHORITIES[4],
];

fn oracle_price(context: &PriceContext, direction: bool) -> u64 {
    if direction {
        context.ask_price()
    } else {
        context.bid_price()
    }
}

#[inline(always)]
fn execution_price_deviation(
    limit: u64,
    bid_decimals: u8,
    amount_in: u64,
    amount_out: u64,
    oracle_price: u64,
    is_user_bid: bool,
) -> bool {
    if limit == 0 {
        return false;
    }

    let base_unit = match 10u128.checked_pow(u32::from(bid_decimals)) {
        Some(unit) => unit,
        None => return true,
    };
    let (exec_num, exec_denom): (u128, u128) = if is_user_bid {
        let num = match u128::from(amount_in).checked_mul(base_unit) {
            Some(num) => num,
            None => return true,
        };
        (num, u128::from(amount_out))
    } else {
        let num = match u128::from(amount_out).checked_mul(base_unit) {
            Some(num) => num,
            None => return true,
        };
        (num, u128::from(amount_in))
    };

    let oracle_times_denom = match u128::from(oracle_price).checked_mul(exec_denom) {
        Some(value) => value,
        None => return true,
    };
    let difference = if exec_num > oracle_times_denom {
        exec_num.saturating_sub(oracle_times_denom)
    } else {
        oracle_times_denom.saturating_sub(exec_num)
    };
    let lhs = match difference.checked_mul(1_000_000) {
        Some(value) => value,
        None => return true,
    };
    let rhs = match u128::from(limit)
        .checked_mul(u128::from(oracle_price))
        .and_then(|value| value.checked_mul(exec_denom))
    {
        Some(value) => value,
        None => return true,
    };
    lhs > rhs
}

/// Swaps tokens with route authority checks and route-aware pricing.
pub fn swap_with_route(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(7, 4);
    let [
        user,
        market_authority,
        bid_user_account,
        ask_user_account,
        bid_vault,
        ask_vault,
        bid_mint_account,
        ask_mint_account,
        oracle_account,
        bloom_account,
        instructions,
        bid_program_account,
        ask_program_account,
        ..
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_token_program(bid_program_account)?;
    validate_token_program(ask_program_account)?;
    validate_instructions_sysvar(instructions)?;

    if !user.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    let extra_accounts = accounts.len() - 13;
    let route =
        decode_quote_route(instructions).map_err(|_| ProgramError::InvalidInstructionData)?;
    let route_class = route.classification;
    let route_authority = route.authority.unwrap_or(*user.key());
    let mut auxiliary_authority: Result<&AccountInfo, u64> = Err(0);
    if extra_accounts != 0 {
        let route_program = &accounts[13];
        let is_jito = route_program.key() == &JITO_ROUTE_PROGRAM;
        let is_foreign_program = route_program.key() != &pubkey::ID;
        if is_foreign_program || &PERMITTED_ROUTE_AUTHORITIES[0] != &route_authority {
            if is_foreign_program
                || (&PERMITTED_ROUTE_AUTHORITIES[1] != &route_authority
                    && &PERMITTED_ROUTE_AUTHORITIES[2] != &route_authority
                    && !PERMITTED_ROUTE_AUTHORITIES[3..].contains(&route_authority))
            {
                if !is_jito {
                    return Err(GoonFiError::InvalidRouteAuthorization.into());
                }
            }
        }
        if extra_accounts != 1 {
            let account = &accounts[14];
            if account.key() != &AUXILIARY_ROUTE_AUTHORITY {
                return Err(GoonFiError::InvalidRouteAuthorization.into());
            }
            auxiliary_authority = Ok(account);
        }
    }

    let market_account = &accounts[1];
    let mut market = MarketConfigurationState::borrow_account_mut(market_account)?;
    let market_key = market_account.key();
    let bid_program = bid_program_account.key();
    if &market.as_body().bid_token_program != bid_program {
        return Err(ProgramError::IncorrectProgramId);
    }
    let ask_program = ask_program_account.key();
    if &market.as_body().ask_token_program != ask_program {
        return Err(ProgramError::IncorrectProgramId);
    }
    if bid_vault.key() != &market.as_body().bid_vault {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        bid_vault,
        &market.as_body().bid_mint,
        market_key,
        bid_program,
    )?;
    if ask_vault.key() != &market.as_body().ask_vault {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        ask_vault,
        &market.as_body().ask_mint,
        market_key,
        ask_program,
    )?;
    if oracle_account.key() != &market.as_body().oracle {
        return Err(GoonFiError::OracleAccountMismatch.into());
    }
    validate_bloom_filter_account(bloom_account)?;
    if market.as_body().disabled != 0 {
        return Err(GoonFiError::MarketDisabled.into());
    }

    let (arguments, prefer_jupiter) = decode_swap_arguments(instruction_data)?;
    if arguments.amount == 0 {
        return Err(GoonFiError::ZeroSwapAmount.into());
    }
    let signed_class = classify_signed_route(instructions, route_class as u32)?;
    let bloom_member = bloom_account_contains_key(bloom_account, &route_authority)?;
    let bloom_mode = if bloom_member { 1 } else { 2 };
    let mut class = classify_quote_route(
        user.key(),
        prefer_jupiter,
        signed_class,
        bloom_mode,
        &route,
        instructions,
        &market.as_body().bid_mint,
        &market.as_body().ask_mint,
    )?;
    let auxiliary_is_signer = if auxiliary_authority.is_err() {
        false
    } else {
        auxiliary_authority.unwrap().is_signer()
    };
    let recognized_signer = transaction_has_recognized_signer(instructions)?;
    if !matches!(
        class,
        PricingClass::Authorized | PricingClass::RecognizedSigner
    ) {
        if matches!(route_class, RouteClass::Pro) {
            class = PricingClass::Authorized;
        }
        if extra_accounts != 0 && matches!(route_class, RouteClass::Pro) {
            class = if recognized_signer || auxiliary_is_signer {
                PricingClass::ProWithAuxiliarySigner
            } else {
                PricingClass::Pro
            };
        }
    }

    validate_token_account(
        bid_user_account,
        &market.as_body().bid_mint,
        user.key(),
        bid_program,
    )?;
    validate_token_account(
        ask_user_account,
        &market.as_body().ask_mint,
        user.key(),
        ask_program,
    )?;
    let context_key =
        crate::pubkey::AccountPubkey(bytemuck::cast(read_oracle_prefix(oracle_account)?));
    let context = PriceContext(context_key.0);
    let spread = u32::from_le_bytes(context.0[20..24].try_into().unwrap());
    let direction = arguments.direction;
    let (previous, current) = if direction {
        (
            market.as_body().ask_reserve_baseline,
            oracle_price(&context, true),
        )
    } else {
        (
            market.as_body().bid_reserve_baseline,
            oracle_price(&context, false),
        )
    };
    let deviation_limit = market.as_body().reserve_tolerance;
    let oracle_result: ProgramResult = if previous == 0 || deviation_limit == 0 {
        Ok(())
    } else {
        let adverse_move = if direction {
            if current < previous {
                Some(previous.saturating_sub(current))
            } else {
                None
            }
        } else if current > previous {
            Some(current.saturating_sub(previous))
        } else {
            None
        };
        if let Some(deviation) = adverse_move {
            if u128::from(deviation) * 1_000_000
                > u128::from(deviation_limit) * u128::from(previous)
            {
                Err(GoonFiError::ReservePriceToleranceExceeded.into())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    };
    oracle_result?;

    let bump: [u8; 1] = [market.as_body().bump];
    let market_seed = market.as_body().seed.to_le_bytes();
    let bid_mint = market.as_body().bid_mint;
    let ask_mint = market.as_body().ask_mint;
    let inventory = if &market.as_body().bid_token_program == &TOKEN_PROGRAM_ID {
        pinocchio_token::state::TokenAccount::from_account_info(bid_vault)
            .map_err(|_| ProgramError::InvalidAccountData)
            .map(|token| token.amount())
    } else if &market.as_body().bid_token_program == &TOKEN_2022_PROGRAM_ID {
        pinocchio_token_2022::state::TokenAccount::from_account_info(bid_vault)
            .map_err(|_| ProgramError::InvalidAccountData)
            .map(|token| token.amount())
    } else {
        Err(ProgramError::InvalidAccountData)
    }?;
    let slot = get_clock()?.slot;
    let pricing_market = MarketPricingView::from_account_data(bytemuck::bytes_of(&*market))?;
    let quote = {
        let quote_context = PriceContext(context_key.0);
        pricing_market.calculate_quote_price(
            arguments.amount,
            arguments.other_amount_threshold,
            slot,
            arguments.direction,
            inventory,
            class,
            bytemuck::pod_read_unaligned::<oracle_asm::Oracle>(&quote_context.0),
        )
    }?;
    let reference_price = oracle_price(&context, arguments.direction);
    let limit = market.as_body().quote_limit;
    let price_check: ProgramResult = if execution_price_deviation(
        limit,
        market.as_body().bid_decimals,
        arguments.amount,
        quote.quote_amount,
        reference_price,
        arguments.direction,
    ) {
        Err(GoonFiError::ExecutionPriceLimitCheckFailed.into())
    } else {
        Ok(())
    };
    price_check?;

    let update_slot = context.update_slot();
    let marker = (slot as u8) ^ (class as u8) ^ 0xa7;
    {
        let spread_value = u64::from(spread);
        let message = std::format!(
            "{:016x}-{:016x}-{:016x}-{:02x}",
            slot ^ spread_value,
            quote.quote_amount ^ slot,
            slot ^ update_slot,
            marker,
        );
        let mut logger = LogBuffer::default();
        logger.append(message.as_str()).log();
    }

    let side = arguments.direction;
    let last_slot = if side {
        market.ask_update_slot_mut()
    } else {
        market.bid_update_slot_mut()
    };
    *last_slot = quote.price_update_slot;
    let cumulative = if side {
        market.ask_quantity_mut()
    } else {
        market.bid_quantity_mut()
    };
    *cumulative = quote.next_quantity;
    let reference = if side {
        market.ask_reserve_baseline_mut()
    } else {
        market.bid_reserve_baseline_mut()
    };
    let mut reserve_price = context.ask_price();
    if !side {
        reserve_price = context.bid_price();
    }
    *reference = reserve_price;
    let input_user = if side {
        ask_user_account
    } else {
        bid_user_account
    };
    let input_vault = if side { ask_vault } else { bid_vault };
    let input_program_account = if side {
        ask_program_account
    } else {
        bid_program_account
    };
    let output_vault = if side { bid_vault } else { ask_vault };
    let output_user = if side {
        bid_user_account
    } else {
        ask_user_account
    };
    let output_program_account = if side {
        bid_program_account
    } else {
        ask_program_account
    };
    let input_mint = if side {
        ask_mint_account
    } else {
        bid_mint_account
    };
    let output_mint = if side {
        bid_mint_account
    } else {
        ask_mint_account
    };

    *market.smoothed_ask_adjustment_mut() = quote.smoothed_ask_adjustment;
    *market.smoothing_slot_mut() = quote.smoothing_slot;
    *market.smoothed_bid_adjustment_mut() = quote.smoothed_bid_adjustment;
    let bid_decimals = market.as_body().bid_decimals;
    let ask_decimals = market.as_body().ask_decimals;
    drop(market);
    let output_decimals = if side { bid_decimals } else { ask_decimals };
    let input_program = input_program_account.key();

    if input_program == &TOKEN_2022_PROGRAM_ID {
        let input_decimals = if side { ask_decimals } else { bid_decimals };
        pinocchio_token_2022::instructions::TransferChecked {
            from: input_user,
            mint: input_mint,
            to: input_vault,
            authority: user,
            amount: arguments.amount,
            decimals: input_decimals,
            token_program: input_program,
        }
        .invoke()?;
    } else {
        pinocchio_token_2022::instructions::Transfer {
            from: input_user,
            to: input_vault,
            authority: user,
            amount: arguments.amount,
            token_program: input_program,
        }
        .invoke()?;
    }

    let seeds = [
        Seed::from(b"market_v2"),
        Seed::from(&bid_mint),
        Seed::from(&ask_mint),
        Seed::from(&market_seed),
        Seed::from(&bump),
    ];
    let output_program = output_program_account.key();
    if output_program == &TOKEN_2022_PROGRAM_ID {
        let signers = [Signer::from(&seeds)];
        pinocchio_token_2022::instructions::TransferChecked {
            from: output_vault,
            mint: output_mint,
            to: output_user,
            authority: market_authority,
            amount: quote.quote_amount,
            decimals: output_decimals,
            token_program: output_program,
        }
        .invoke_signed(&signers)?;
    } else {
        let signers = [Signer::from(&seeds)];
        pinocchio_token_2022::instructions::Transfer {
            from: output_vault,
            to: output_user,
            authority: market_authority,
            amount: quote.quote_amount,
            token_program: output_program,
        }
        .invoke_signed(&signers)?;
    }

    if !COMPUTE_EXEMPT_SIGNERS.contains(user.key()) {
        let mut i = 0u64;
        while i < 7000 {
            core::hint::black_box(i);
            i += 1;
        }
    }
    Ok(())
}
