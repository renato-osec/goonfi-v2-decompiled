use crate::{
    account_info::{validate_system_program, AccountInfo},
    internal::{ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
    state::{
        market_v2::MarketConfigurationState,
        whitelisted_key_registry::{
            Whitelist, MAX_WHITELIST_PROGRAMS, WHITELIST_REGISTRY_ID,
        },
    },
};
use borsh::BorshDeserialize;
use crate::utils::helpers::prologue_padding;

/// The updater can change a market without an entry in the operator registry.
pub(super) const UPDATE_AUTHORITY: Pubkey = [
    0x0d, 0x88, 0x0e, 0xee, 0x7b, 0xa8, 0x04, 0x97, 0x27, 0x12, 0xaf, 0x88, 0x3a, 0x68, 0x7c, 0x6e,
    0xbb, 0x50, 0xd3, 0xe8, 0x54, 0x28, 0x2b, 0x87, 0xac, 0xca, 0x77, 0xfa, 0x01, 0xde, 0xd9, 0xba,
];

pub(super) const MARKET_DISCRIMINATOR: u64 = 0x9a32_5834_352f_bc30;
pub(super) const MARKET_ACCOUNT_LEN: usize = 0x800;
const DISABLE_QUOTES_PERMISSION: u64 = 0x200;
const ENABLE_QUOTES_PERMISSION: u64 = 0x400;
const REFRESH_RESERVES_PERMISSION: u64 = 0x800;
pub(super) const RESTART_SLOT_PERMISSION: u64 = ENABLE_QUOTES_PERMISSION;

fn authorize_market_quotes<const PERMISSION: u64>(accounts: &[AccountInfo]) -> ProgramResult {
    let signer = &accounts[0];
    if !signer.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if signer.key() == &UPDATE_AUTHORITY {
        return Ok(());
    }
    let registry = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .ok_or(ProgramError::IncorrectAuthority)?;
    let denied = Whitelist::borrow_account(registry).map_or(true, |whitelist| {
        let count = whitelist.program_count() as usize;
        !whitelist.programs()[..count.min(MAX_WHITELIST_PROGRAMS)].iter().any(|entry| {
            &entry.key == signer.key() && entry.permissions & PERMISSION != 0
        })
    });
    if denied {
        Err(ProgramError::IncorrectAuthority)
    } else {
        Ok(())
    }
}

#[derive(BorshDeserialize)]
struct ReserveBaselines {
    bid: u64,
    ask: u64,
}

/// Disables quotes from a market until an authorized operator enables them.
pub fn disable_market_quotes(accounts: &[AccountInfo], _instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(14, 4);
    if accounts.len() < 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if let Err(error) = authorize_market_quotes::<DISABLE_QUOTES_PERMISSION>(accounts) {
        return Err(error);
    }

    let market = &accounts[1];
    if market.data_len() != MARKET_ACCOUNT_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    if !market.is_owned_by(&pubkey::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    let mut data = market.try_borrow_mut_data()?;
    if u64::from_le_bytes(data[..8].try_into().unwrap()) != MARKET_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }
    let state =
        bytemuck::from_bytes_mut::<MarketConfigurationState>(&mut data[8..MARKET_ACCOUNT_LEN]);
    state.set_quotes_disabled(true);
    Ok(())
}

/// Enables quotes from a market after an operator review.
pub fn enable_market_quotes(accounts: &[AccountInfo], _instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(8, 1);
    if accounts.len() < 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if let Err(error) = authorize_market_quotes::<ENABLE_QUOTES_PERMISSION>(accounts) {
        return Err(error);
    }

    let market = &accounts[1];
    if market.data_len() != MARKET_ACCOUNT_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    if !market.is_owned_by(&pubkey::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    let mut data = market.try_borrow_mut_data()?;
    if u64::from_le_bytes(data[..8].try_into().unwrap()) != MARKET_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }
    let state =
        bytemuck::from_bytes_mut::<MarketConfigurationState>(&mut data[8..MARKET_ACCOUNT_LEN]);
    state.set_quotes_disabled(false);
    Ok(())
}

/// Refreshes the reserve baselines used by the quote calculation.
pub fn refresh_market(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(7, 1);
    if accounts.len() < 3 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if let RefreshAuthorization::Denied = authorize_refresh_market(accounts) {
        return Err(ProgramError::IncorrectAuthority);
    }

    validate_system_program(&accounts[2])?;

    let mut state = MarketConfigurationState::borrow_account_mut(&accounts[1])?;
    let baselines = ReserveBaselines::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    *state.bid_reserve_baseline_mut() = baselines.bid;
    *state.ask_reserve_baseline_mut() = baselines.ask;
    Ok(())
}

enum RefreshAuthorization {
    Allowed,
    Denied,
}

fn authorize_refresh_market(accounts: &[AccountInfo]) -> RefreshAuthorization {
    let signer = &accounts[0];
    if !signer.is_signer() {
        return RefreshAuthorization::Denied;
    }
    if signer.key() == &UPDATE_AUTHORITY {
        return RefreshAuthorization::Allowed;
    }
    let Some(registry) = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID) else {
        return RefreshAuthorization::Denied;
    };
    let denied = Whitelist::borrow_account(registry).map_or(true, |whitelist| {
        let entries = whitelist.programs();
        let count = (whitelist.program_count() as usize).min(entries.len());
        !entries[..count].iter().any(|entry| {
            &entry.key == signer.key()
                && entry.permissions & REFRESH_RESERVES_PERMISSION != 0
        })
    });
    if denied {
        RefreshAuthorization::Denied
    } else {
        RefreshAuthorization::Allowed
    }
}
