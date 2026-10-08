use crate::utils::helpers::prologue_padding;
use crate::{
    account_info::{validate_token_program, AccountInfo},
    extension::{Seed, Signer},
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
    state::{
        market_v2::MarketConfigurationState,
        whitelisted_key_registry::{Whitelist, MAX_WHITELIST_PROGRAMS, WHITELIST_REGISTRY_ID},
    },
};
use borsh::BorshDeserialize;
use goonfi_account_validation::validate_token_account;
use pinocchio::sysvars::{clock::Clock, Sysvar};

use super::validate_liquidity_token_account;

const NATIVE_MINT: Pubkey = [
    0x06, 0x9b, 0x88, 0x57, 0xfe, 0xab, 0x81, 0x84, 0xfb, 0x68, 0x7f, 0x63, 0x46, 0x18, 0xc0, 0x35,
    0xda, 0xc4, 0x39, 0xdc, 0x1a, 0xeb, 0x3b, 0x55, 0x98, 0xa0, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x01,
];

const LIMITED_WITHDRAWAL_COOLDOWN: i64 = 7_200;
const MAX_LIMITED_WITHDRAWAL: u64 = 15_000_000_000;
const UNRESTRICTED_WITHDRAW_PERMISSION: u64 = 4;
const LIMITED_WITHDRAW_PERMISSION: u64 = 0x2000;

fn lacks_withdraw_permission<const PERMISSION: u64>(
    whitelist: &Whitelist,
    signer_key: &Pubkey,
) -> bool {
    let count = whitelist.program_count().min(MAX_WHITELIST_PROGRAMS as u32) as usize;
    whitelist.programs()[..count]
        .iter()
        .all(|entry| &entry.key != signer_key || entry.permissions & PERMISSION == 0)
}

fn validate_unrestricted_withdraw_authority(
    accounts: &[AccountInfo],
    signer: &AccountInfo,
) -> ProgramResult {
    if !signer.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if signer.key() == &super::LIQUIDITY_AUTHORITY {
        return Ok(());
    }
    let whitelist = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .and_then(|account| Whitelist::borrow_account(account).ok());
    let denied = whitelist.is_none_or(|whitelist| {
        lacks_withdraw_permission::<UNRESTRICTED_WITHDRAW_PERMISSION>(&whitelist, signer.key())
    });
    if denied {
        Err(ProgramError::IncorrectAuthority)
    } else {
        Ok(())
    }
}

fn validate_limited_withdraw_authority(
    accounts: &[AccountInfo],
    signer: &AccountInfo,
) -> ProgramResult {
    if !signer.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if signer.key() == &super::LIQUIDITY_AUTHORITY {
        return Ok(());
    }
    let whitelist = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .and_then(|account| Whitelist::borrow_account(account).ok());
    let Some(whitelist) = whitelist else {
        return Err(ProgramError::IncorrectAuthority);
    };
    let allowed =
        !lacks_withdraw_permission::<LIMITED_WITHDRAW_PERMISSION>(&whitelist, signer.key());
    if pinocchio::hint::unlikely(allowed) {
        drop(whitelist);
        return Ok(());
    }
    pinocchio::hint::cold_path();
    Err(ProgramError::IncorrectAuthority)
}

fn enforce_limited_withdrawal(
    market: &AccountInfo,
    bid_amount: u64,
    ask_amount: u64,
) -> ProgramResult {
    let timestamp = Clock::get()?.unix_timestamp;
    let mut market_state = MarketConfigurationState::borrow_account_mut(market)?;
    if ask_amount != 0 {
        return Err(GoonFiError::InvalidLimitedWithdrawal.into());
    }
    let last = market_state.last_limited_withdrawal_timestamp();
    if market_state.bid_mint_key() != &NATIVE_MINT || bid_amount == 0 {
        return Err(GoonFiError::InvalidLimitedWithdrawal.into());
    }
    if bid_amount > MAX_LIMITED_WITHDRAWAL {
        return Err(GoonFiError::LimitedWithdrawalAmountTooLarge.into());
    }
    if last != 0 && timestamp.saturating_sub(last) < LIMITED_WITHDRAWAL_COOLDOWN {
        return Err(GoonFiError::LimitedWithdrawalCooldown.into());
    }
    *market_state.last_limited_withdrawal_timestamp_mut() = timestamp;
    Ok(())
}

#[derive(BorshDeserialize)]
struct WithdrawAmounts {
    bid_amount: u64,
    ask_amount: u64,
}

/// Withdraws the requested bid and ask amounts from the market vaults.
pub fn withdraw(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(9, 3);
    if accounts.len() < 10 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let market_account = &accounts[1];
    let WithdrawAmounts {
        bid_amount,
        ask_amount,
    } = match WithdrawAmounts::try_from_slice(instruction_data) {
        Ok(amounts) => Ok(amounts),
        Err(_) => Err(ProgramError::InvalidInstructionData),
    }?;
    let signer = &accounts[0];
    let bid_destination = &accounts[2];
    let ask_destination = &accounts[3];
    let bid_vault = &accounts[4];
    let ask_vault = &accounts[5];
    let bid_mint_account = &accounts[6];
    let ask_mint_account = &accounts[7];
    let bid_token_program = &accounts[8];
    let ask_token_program = &accounts[9];
    if !signer.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if validate_unrestricted_withdraw_authority(accounts, signer).is_err() {
        validate_limited_withdraw_authority(accounts, signer)?;
        enforce_limited_withdrawal(market_account, bid_amount, ask_amount)?;
    }

    // Validate the ask program only after the bid program passes.
    let first_program = validate_token_program(bid_token_program);
    if first_program.is_ok() {
        validate_token_program(ask_token_program)?;
    }
    first_program?;

    let market = MarketConfigurationState::borrow_account(market_account)?;
    let market_key = market_account.key();
    if bid_vault.key() != market.bid_vault_key() {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        bid_vault,
        market.bid_mint_key(),
        market_key,
        bid_token_program.key(),
    )?;
    if ask_vault.key() != market.ask_vault_key() {
        return Err(ProgramError::InvalidAccountData);
    }
    validate_token_account(
        ask_vault,
        market.ask_mint_key(),
        market_key,
        ask_token_program.key(),
    )?;

    let whitelist = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .and_then(|account| Whitelist::borrow_account(account).ok());
    let bid_mint = market.bid_mint_key();
    let ask_mint = market.ask_mint_key();
    validate_liquidity_token_account(
        bid_destination,
        bid_mint,
        whitelist.as_deref(),
        bid_token_program.key(),
    )?;
    validate_liquidity_token_account(
        ask_destination,
        ask_mint,
        whitelist.as_deref(),
        ask_token_program.key(),
    )?;

    let bump = [market.bump()];
    let market_id = market.seed();
    let bid_mint = *bid_mint;
    let ask_mint = *ask_mint;
    let bid_decimals = market.bid_decimals();
    let ask_decimals = market.ask_decimals();
    let seeds = [
        Seed::from(b"market_v2"),
        Seed::from(&bid_mint),
        Seed::from(&ask_mint),
        Seed::from(&market_id),
        Seed::from(&bump),
    ];
    {
        let source = bid_vault;
        let destination = bid_destination;
        let authority = market_account;
        let token_program = bid_token_program;
        let amount = bid_amount;
        let decimals = bid_decimals;
        let is_token_2022 = token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID;
        if !is_token_2022 {
            let signers = [Signer::from(&seeds)];
            pinocchio_token_2022::instructions::Transfer {
                from: source,
                to: destination,
                authority,
                amount,
                token_program: token_program.key(),
            }
            .invoke_signed(&signers)?;
        } else {
            let mint = bid_mint_account;
            let signers = [Signer::from(&seeds)];
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
        }
    }
    // Either token variant yields one result after its signer seeds leave scope.
    let ask_transfer_result: ProgramResult = 'ask_transfer: {
        let source = ask_vault;
        let mint = ask_mint_account;
        let destination = ask_destination;
        let authority = market_account;
        let token_program = ask_token_program;
        let amount = ask_amount;
        let decimals = ask_decimals;
        let is_token_2022 = token_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID;
        if !is_token_2022 {
            let signers = [Signer::from(&seeds)];
            let result = pinocchio_token_2022::instructions::Transfer {
                from: source,
                to: destination,
                authority,
                amount,
                token_program: token_program.key(),
            }
            .invoke_signed(&signers);
            if let Err(error) = result {
                break 'ask_transfer Err(error);
            }
        } else {
            let signers = [Signer::from(&seeds)];
            let result = pinocchio_token_2022::instructions::TransferChecked {
                from: source,
                mint,
                to: destination,
                authority,
                amount,
                decimals,
                token_program: token_program.key(),
            }
            .invoke_signed(&signers);
            if let Err(error) = result {
                break 'ask_transfer Err(error);
            }
        }
        Ok(())
    };
    if let Err(error) = ask_transfer_result {
        return Err(error);
    }
    Ok(())
}
