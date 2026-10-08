use crate::{
    account_info::AccountInfo,
    internal::{ProgramError, ProgramResult},
    pubkey::Pubkey,
    state::market_v2::{MarketConfigurationState, PricingTier},
    state::whitelisted_key_registry::{
        Whitelist, MAX_WHITELIST_PROGRAMS, WHITELIST_REGISTRY_ID,
    },
};
use crate::utils::helpers::prologue_padding;

pub(crate) use crate::state::market_v2::{apply_market_configuration, MarketConfigurationInput};

pub(super) const MARKET_ADMINISTRATOR: Pubkey = [
    0x0d, 0x88, 0x0e, 0xee, 0x7b, 0xa8, 0x04, 0x97, 0x27, 0x12, 0xaf, 0x88, 0x3a, 0x68, 0x7c, 0x6e,
    0xbb, 0x50, 0xd3, 0xe8, 0x54, 0x28, 0x2b, 0x87, 0xac, 0xca, 0x77, 0xfa, 0x01, 0xde, 0xd9, 0xba,
];

fn require_registry_permission(accounts: &[AccountInfo], permission: u64) -> ProgramResult {
    let authority = &accounts[0];
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
    let allowed = {
        let state = Whitelist::borrow_account(registry)
            .map_err(|_| ProgramError::IncorrectAuthority)?;
        let count = state.program_count() as usize;
        let programs = state.programs();
        programs[..count.min(MAX_WHITELIST_PROGRAMS)]
            .iter()
            .find(|entry| &entry.key == authority.key() && entry.permissions & permission != 0)
            .is_some()
    };
    allowed
        .then_some(())
        .ok_or(ProgramError::IncorrectAuthority)
}

/// Initial curve parameters and the PDA bumps for a new market.
#[derive(borsh::BorshDeserialize)]
pub struct InitializeMarketParameters {
    pub market_id: u64,
    pub near_bid_denominator: u64,
    pub near_ask_denominator: u64,
    pub inventory_knee: u64,
    pub far_bid_denominator: u64,
    pub far_ask_denominator: u64,
    pub minimum_quote_threshold: u64,
    pub inventory_limit: u64,
    pub bid_thresholds: [u64; 10],
    pub ask_thresholds: [u64; 10],
    pub initial_tier: PricingTier,
    pub class_0: u8,
    pub class_1: u8,
    pub class_2: u8,
    pub class_3: u8,
    pub class_4: u8,
    pub class_5: u8,
    pub class_6: u8,
    pub class_7: u8,
    pub class_8: u8,
    pub class_9: u8,
    pub class_10: u8,
    pub class_11: u8,
    pub class_12: u8,
    pub class_13: u8,
    pub class_14: u8,
    pub class_15: u8,
    pub maximum_quote_age: u8,
    pub grace_slots: u8,
    pub bid_decimals: u8,
    pub ask_decimals: u8,
    pub market_bump: u8,
    pub bid_vault_bump: u8,
    pub ask_vault_bump: u8,
    pub bid_rate_millionths: u64,
    pub ask_rate_millionths: u64,
    pub smoothing_period: u64,
    pub smoothing_interval: u64,
    pub smoothing_mode: u8,
    pub disable_staleness_adjustment: u8,
    pub quote_flags: u8,
}

/// Replaces one pricing tier under an administrator or delegated permission.
#[inline(never)]
pub(crate) fn configure_pricing_tier(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    prologue_padding!(4, 2);
    let [_, market_account, system_program, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if require_registry_permission(accounts, 0x20).is_err() {
        return Err(ProgramError::IncorrectAuthority);
    }
    goonfi_account_validation::validate_system_program(system_program)?;
    let mut market = MarketConfigurationState::borrow_account_mut(market_account)?;

    // The decoded index selects the tier replaced below.
    let mut index = 0;
    let tier = <PricingTierUpdate as borsh::BorshDeserialize>::try_from_slice(data)
        .map(|update| {
            index = update.index;
            update.tier
        })
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let tiers = market.pricing_tiers_mut();
    tiers.update_pricing_tier(index, &tier)?;
    Ok(())
}

/// Handles the pricing tier update instruction.
pub fn set_pricing_tier(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    configure_pricing_tier(accounts, data)
}

/// Applies a complete market configuration under administrator or delegated authority.
pub fn configure_market(accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    prologue_padding!(8, 2);
    let [authority, market_account, system_program, _, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    (|| -> ProgramResult {
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
        let state = Whitelist::borrow_account(registry)
            .map_err(|_| ProgramError::IncorrectAuthority)?;
        let count = state.program_count() as usize;
        let programs = state.programs();
        // A matching delegated entry ends the permission scan.
        let denial = programs[..count.min(MAX_WHITELIST_PROGRAMS)]
            .iter()
            .try_for_each(|entry| {
                if &entry.key == authority.key() && entry.permissions & 0x8 != 0 {
                    None
                } else {
                    Some(())
                }
            })
            .map(|_| ProgramError::IncorrectAuthority);
        drop(state);
        denial.map_or(Ok(()), Err)
    })()
    .map_err(|_| ProgramError::IncorrectAuthority)?;
    goonfi_account_validation::validate_system_program(system_program)?;
    let mut market = MarketConfigurationState::borrow_account_mut(market_account)?;
    let configuration = <MarketConfigurationInput as borsh::BorshDeserialize>::try_from_slice(input)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    apply_market_configuration(&mut market, &configuration)?;
    Ok(())
}

/// Handles the complete market configuration instruction.
pub fn update_configuration(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    configure_market(accounts, data)
}

#[derive(borsh::BorshDeserialize)]
struct PricingTierUpdate {
    index: u8,
    tier: PricingTier,
}
