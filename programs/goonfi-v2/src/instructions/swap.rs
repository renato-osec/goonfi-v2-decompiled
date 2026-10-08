use super::swap_with_route::COMPUTE_EXEMPT_SIGNERS;
use crate::utils::helpers::prologue_padding;
use crate::{
    account_info::{
        read_oracle_prefix, validate_bloom_filter_account, validate_token_program, AccountInfo,
    },
    extension::{invoke_signed, AccountMeta, Instruction, Seed, Signer},
    internal::{sysvars::get_clock, GoonFiError, ProgramError, ProgramResult},
    logger::LogBuffer,
    pubkey::{self, Pubkey},
    state::{
        decode_swap_arguments,
        market_v2::{
            MarketConfigurationState, MarketPricingView, PriceContext, PriceContextFields,
        },
    },
    utils::cpi::{classify_quote_route, decode_quote_route},
};
use goonfi_account_validation::{validate_instructions_sysvar, validate_token_account};

const JITO_ROUTE_PROGRAM: Pubkey = [
    0xfc, 0xd1, 0x3d, 0xa0, 0x41, 0x55, 0x90, 0x13, 0xcf, 0x4a, 0xeb, 0xa1, 0xa8, 0x4e, 0xd7, 0x02,
    0x3a, 0xd4, 0xff, 0x21, 0xa8, 0x47, 0x47, 0xff, 0x3b, 0x05, 0xab, 0x30, 0xfa, 0x82, 0x07, 0xc3,
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

fn transfer_data(amount: u64) -> [u8; 9] {
    let mut data = [0u8; 9];
    data[0] = 3;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data
}

fn transfer_metas<'a>(
    source: &'a AccountInfo,
    destination: &'a AccountInfo,
    authority: &'a AccountInfo,
) -> [AccountMeta<'a>; 3] {
    [
        AccountMeta::writable(source.key()),
        AccountMeta::writable(destination.key()),
        AccountMeta::readonly_signer(authority.key()),
    ]
}

fn scaled_quote_amounts(
    scale: u128,
    direction: bool,
    requested_amount: u64,
    outcome_quote_amount: u64,
) -> Result<(u128, u128), ProgramError> {
    let price_limit_error = ProgramError::from(GoonFiError::ExecutionPriceLimitCheckFailed);
    if direction {
        let scaled_base = (requested_amount as u128)
            .checked_mul(scale)
            .ok_or(price_limit_error)?;
        Ok((scaled_base, outcome_quote_amount as u128))
    } else {
        let scaled_base = (outcome_quote_amount as u128)
            .checked_mul(scale)
            .ok_or(price_limit_error)?;
        Ok((scaled_base, requested_amount as u128))
    }
}

fn quote_deviation_within_limit(
    difference: u128,
    quote_limit: u64,
    price: u128,
    quote_amount: u128,
) -> Option<()> {
    let deviation = difference.checked_mul(1_000_000)?;
    let limit = ((quote_limit as u128) * price).checked_mul(quote_amount)?;
    if deviation > limit {
        return None;
    }
    Some(())
}

/// Exchanges the trader's tokens with the market vaults at the routed quote.
pub fn swap(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(9, 4);
    let [
        user,
        market_account,
        bid_user_account,
        ask_user_account,
        bid_vault,
        ask_vault,
        bid_mint_account,
        ask_mint_account,
        oracle,
        bloom_account,
        instructions_sysvar,
        bid_token_program,
        ask_token_program,
        ..
    ] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_token_program(bid_token_program)?;
    validate_token_program(ask_token_program)?;
    validate_instructions_sysvar(instructions_sysvar)?;

    if !user.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }

    let route = decode_quote_route(instructions_sysvar)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let route_authority = route.authority.unwrap_or(*user.key());
    if let Some(extra_account) = accounts.get(13) {
        let is_jito = extra_account.key() == &JITO_ROUTE_PROGRAM;
        let is_program = extra_account.key() == &pubkey::ID;
        if !(is_program && PERMITTED_ROUTE_AUTHORITIES.contains(&route_authority)) && !is_jito {
            return Err(GoonFiError::InvalidRouteAuthorization.into());
        }
    }

    let mut market_data = MarketConfigurationState::borrow_account_mut(market_account)?;
    let market_key = market_account.key();
    let market = market_data.as_body_mut();

    if &market.bid_token_program != bid_token_program.key()
        || &market.ask_token_program != ask_token_program.key()
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    if bid_vault.key() != &market.bid_vault {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        bid_vault,
        &market.bid_mint,
        market_key,
        bid_token_program.key(),
    )?;
    if ask_vault.key() != &market.ask_vault {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        ask_vault,
        &market.ask_mint,
        market_key,
        ask_token_program.key(),
    )?;
    if oracle.key() != &market.oracle {
        return Err(GoonFiError::OracleAccountMismatch.into());
    }
    validate_bloom_filter_account(bloom_account)?;
    if market.disabled != 0 {
        return Err(GoonFiError::MarketDisabled.into());
    }

    let (request, trailing_flag) = decode_swap_arguments(instruction_data)?;
    if request.amount == 0 {
        return Err(GoonFiError::ZeroSwapAmount.into());
    }
    let signed_route_class = crate::internal::instruction_sysvar::classify_signed_route(
        instructions_sysvar,
        route.classification as u32,
    )?;
    let is_known =
        super::bloom_queries::bloom_account_contains_key(bloom_account, &route_authority)?;
    let route_class = classify_quote_route(
        user.key(),
        trailing_flag,
        signed_route_class,
        if is_known { 1 } else { 2 },
        &route,
        instructions_sysvar,
        &market.bid_mint,
        &market.ask_mint,
    )?;

    validate_token_account(
        bid_user_account,
        &market.bid_mint,
        user.key(),
        bid_token_program.key(),
    )?;
    validate_token_account(
        ask_user_account,
        &market.ask_mint,
        user.key(),
        ask_token_program.key(),
    )?;
    let context_bytes = crate::pubkey::AccountPubkey(bytemuck::cast(read_oracle_prefix(oracle)?));
    let price_update_slot = u32::from_le_bytes(context_bytes.0[16..20].try_into().unwrap());
    let spread_multiplier = u32::from_le_bytes(context_bytes.0[20..24].try_into().unwrap());
    let (reference, price) = if request.direction {
        (
            market.ask_reserve_baseline,
            u64::from_le_bytes(context_bytes.0[8..16].try_into().unwrap()),
        )
    } else {
        (
            market.bid_reserve_baseline,
            u64::from_le_bytes(context_bytes.0[..8].try_into().unwrap()),
        )
    };
    let price_check: ProgramResult = if reference == 0 || market.reserve_tolerance == 0 {
        Ok(())
    } else {
        let adverse_move = if request.direction {
            if price < reference {
                Some(reference.saturating_sub(price))
            } else {
                None
            }
        } else if price > reference {
            Some(price.saturating_sub(reference))
        } else {
            None
        };
        if let Some(deviation) = adverse_move {
            let observed = (deviation as u128) * 1_000_000;
            let allowed = (market.reserve_tolerance as u128) * (reference as u128);
            if observed > allowed {
                Err(GoonFiError::ReservePriceToleranceExceeded.into())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    };
    price_check?;

    let bump = [market.bump];
    let market_seed = market.seed.to_le_bytes();
    let bid_mint = market.bid_mint;
    let ask_mint = market.ask_mint;

    let vault_amount =
        goonfi_common::helpers::get_token_account_amount(bid_vault, &market.bid_token_program)
            .map_err(|_| ProgramError::InvalidAccountData)?;
    let slot = get_clock()?.slot;
    let bid_price = u64::from_le_bytes(context_bytes.0[..8].try_into().unwrap());
    let outcome = {
        let prefix = PriceContextFields {
            bid_price,
            ask_price: u64::from_le_bytes(context_bytes.0[8..16].try_into().unwrap()),
            update_slot: price_update_slot,
            spread_multiplier,
            timestamp: u64::from_le_bytes(context_bytes.0[24..].try_into().unwrap()),
        };
        let context = PriceContext::from(prefix);
        let pricing_market = MarketPricingView::from_account_data(bytemuck::bytes_of(&*market))?;
        pricing_market.calculate_quote_price(
            request.amount,
            request.other_amount_threshold,
            slot,
            request.direction,
            vault_amount,
            route_class,
            bytemuck::cast(context.0),
        )?
    };
    let ask_price = u64::from_le_bytes(context_bytes.0[8..16].try_into().unwrap());
    let selected_price = if request.direction {
        ask_price
    } else {
        bid_price
    };
    let quote_check: ProgramResult = if market.quote_limit != 0 {
        let price_limit_error = ProgramError::from(GoonFiError::ExecutionPriceLimitCheckFailed);
        let scale = match 10u128.checked_pow(market.bid_decimals as u32) {
            Some(scale) => scale,
            None => return Err(price_limit_error),
        };
        let outside_limit = match scaled_quote_amounts(
            scale,
            request.direction,
            request.amount,
            outcome.quote_amount,
        ) {
            Ok((scaled_base, quote_amount)) => {
                let price = selected_price as u128;
                let priced_quote = price * quote_amount;
                let difference = if scaled_base > priced_quote {
                    scaled_base.saturating_sub(priced_quote)
                } else {
                    priced_quote.saturating_sub(scaled_base)
                };
                quote_deviation_within_limit(difference, market.quote_limit, price, quote_amount)
                    .is_none()
            }
            Err(_) => true,
        };
        let price_check: ProgramResult = if outside_limit {
            Err(price_limit_error)
        } else {
            Ok(())
        };
        price_check
    } else {
        Ok(())
    };
    quote_check?;

    let marker = (slot as u8) ^ (route_class as u8) ^ 0xa7;
    {
        let first = spread_multiplier as u64;
        let second = price_update_slot as u64;
        let message = std::format!(
            "{:016x}-{:016x}-{:016x}-{:02x}",
            slot ^ first,
            outcome.quote_amount ^ slot,
            slot ^ second,
            marker,
        );
        let mut logger = LogBuffer::default();
        logger.append(message.as_str()).log();
    }

    let selected_slot = if request.direction {
        &mut market.ask_update_slot
    } else {
        &mut market.bid_update_slot
    };
    *selected_slot = outcome.price_update_slot;
    let selected_quantity = if request.direction {
        &mut market.ask_quantity
    } else {
        &mut market.bid_quantity
    };
    *selected_quantity = outcome.next_quantity;
    if request.direction {
        market.ask_reserve_baseline = ask_price;
    } else {
        market.bid_reserve_baseline = bid_price;
    }

    let source = if request.direction {
        ask_user_account
    } else {
        bid_user_account
    };
    let input_vault = if request.direction {
        ask_vault
    } else {
        bid_vault
    };
    let output_vault = if request.direction {
        bid_vault
    } else {
        ask_vault
    };
    let input_mint = if request.direction {
        ask_mint_account
    } else {
        bid_mint_account
    };
    let destination = if request.direction {
        bid_user_account
    } else {
        ask_user_account
    };
    let (input_program, output_program) = if request.direction {
        (ask_token_program, bid_token_program)
    } else {
        (bid_token_program, ask_token_program)
    };
    let output_mint = if request.direction {
        bid_mint_account
    } else {
        ask_mint_account
    };

    market.smoothed_bid_adjustment = outcome.smoothed_bid_adjustment;
    market.smoothed_ask_adjustment = outcome.smoothed_ask_adjustment;
    market.smoothing_slot = outcome.smoothing_slot;

    let bid_decimals = market.bid_decimals;
    let ask_decimals = market.ask_decimals;
    drop(market_data);
    let output_decimals = if request.direction {
        bid_decimals
    } else {
        ask_decimals
    };

    {
        let token_program = input_program;
        let mint = input_mint;
        let destination = input_vault;
        let authority = user;
        let amount = request.amount;
        let decimals = if request.direction {
            ask_decimals
        } else {
            bid_decimals
        };
        let signers = &[];
        if token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
            pinocchio_token_2022::instructions::TransferChecked {
                from: source,
                mint,
                to: destination,
                authority,
                amount,
                decimals,
                token_program: token_program.key(),
            }
            .invoke_signed(signers)?;
        } else {
            pinocchio_token_2022::instructions::Transfer {
                from: source,
                to: destination,
                authority,
                amount,
                token_program: token_program.key(),
            }
            .invoke_signed(signers)?;
        }
    }

    let seeds = [
        Seed::from(b"market_v2"),
        Seed::from(&bid_mint),
        Seed::from(&ask_mint),
        Seed::from(&market_seed),
        Seed::from(&bump),
    ];
    {
        let token_program = output_program;
        let source = output_vault;
        let mint = output_mint;
        let authority = market_account;
        let amount = outcome.quote_amount;
        let decimals = output_decimals;
        let signers: [Signer; 1];
        if token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
            signers = [Signer::from(&seeds)];
            pinocchio_token_2022::instructions::TransferChecked {
                from: source,
                mint,
                to: destination,
                authority,
                amount,
                decimals,
                token_program: token_program.key(),
            }
            .invoke_signed(&signers)?;
        } else {
            let metadata = {
                signers = [Signer::from(&seeds)];
                transfer_metas(source, destination, authority)
            };
            let data = transfer_data(amount);
            let instruction = Instruction {
                program_id: token_program.key(),
                data: &data,
                accounts: &metadata,
            };
            invoke_signed(&instruction, &[source, destination, authority], &signers)?;
        }
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
