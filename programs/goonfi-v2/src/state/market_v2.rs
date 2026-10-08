use crate::{account_info::AccountInfo, internal::ProgramError, pubkey::{self, Pubkey}};
use crate::{internal::GoonFiError, utils::helpers::prologue_padding};
use pinocchio::account_info::{Ref, RefMut};
use oracle_asm::Oracle;

/// Accessor view of the market body following its eight-byte discriminator.
/// Offsets in this table start at the body, after the discriminator.
///
/// | Offset | Data |
/// | --- | --- |
/// | 0x000 | seed |
/// | 0x008 | bid quantity |
/// | 0x010 | ask quantity |
/// | 0x018 | bid update slot |
/// | 0x020 | ask update slot |
/// | 0x028 | market authority |
/// | 0x048 | bid mint |
/// | 0x068 | ask mint |
/// | 0x088 | bid vault |
/// | 0x0a8 | ask vault |
/// | 0x0c8 | oracle |
/// | 0x0e8 | seven pricing tiers |
/// | 0x5b8 | bid thresholds |
/// | 0x608 | ask thresholds |
/// | 0x658 | minimum quote threshold |
/// | 0x660 | near bid and ask denominators |
/// | 0x670 | inventory knee |
/// | 0x678 | far bid and ask denominators |
/// | 0x688 | inventory limit |
/// | 0x690 | quote age, grace slots, and class tiers |
/// | 0x6a0 | reserve tolerance |
/// | 0x6a8 | bid reserve baseline |
/// | 0x6b0 | ask reserve baseline |
/// | 0x6b8 | execution quote limit |
/// | 0x6c0 | extended class tier selectors |
/// | 0x6d0 | smoothing period and interval |
/// | 0x6e0 | smoothed bid adjustment |
/// | 0x6e8 | smoothed ask adjustment |
/// | 0x6f0 | smoothing slot |
/// | 0x6f8 | smoothing mode, staleness toggle, quote flags |
/// | 0x710 | last limited withdrawal |
/// | 0x718 | last restart slot |
/// | 0x7b0 | market and vault bumps |
/// | 0x7b4 | bid decimals |
/// | 0x7b5 | ask decimals |
/// | 0x7b6 | quotes disabled |
/// | 0x7b8 | bid token program |
/// | 0x7d8 | ask token program |
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct MarketV2([u8; Self::MINIMUM_LENGTH]);

pub const MARKET_ACCOUNT_LEN: usize = 8 + MarketV2::MINIMUM_LENGTH;
pub const MARKET_DISCRIMINATOR_BYTES: [u8; 8] = 0x9a325834352fbc30u64.to_le_bytes();

// The aligned byte array covers the complete market body.
unsafe impl bytemuck::Zeroable for MarketV2 {} // SAFETY: Every zero bit pattern is valid.
unsafe impl bytemuck::Pod for MarketV2 {} // SAFETY: The byte array has no padding or invalid bit patterns.

/// The 32-byte oracle context passed to pricing.
/// Direct swaps construct it from fields; routed swaps use the account prefix.
pub struct PriceContext(pub [u8; 32]);

/// Named direct-swap view of the 32-byte oracle prefix.
///
/// | Offset | Field |
/// | --- | --- |
/// | 0x00 | bid price, u64 |
/// | 0x08 | ask price, u64 |
/// | 0x10 | update slot, u32 |
/// | 0x14 | spread multiplier, u32 |
/// | 0x18 | timestamp, u64 |
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PriceContextFields {
    pub bid_price: u64,
    pub ask_price: u64,
    pub update_slot: u32,
    pub spread_multiplier: u32,
    pub timestamp: u64,
}

// The integer fields cover the complete oracle record without padding.
unsafe impl bytemuck::Zeroable for PriceContextFields {} // SAFETY: Every field accepts zero.
unsafe impl bytemuck::Pod for PriceContextFields {} // SAFETY: All fields accept any bit pattern.

impl From<PriceContextFields> for PriceContext {
    // The C layout preserves each oracle field offset during the cast.
    fn from(fields: PriceContextFields) -> Self {
        Self(bytemuck::cast(fields))
    }
}

impl PriceContext {
    pub fn bid_price(&self) -> u64 {
        self.price(false)
    }

    pub fn ask_price(&self) -> u64 {
        self.price(true)
    }

    fn price(&self, direction: bool) -> u64 {
        let offset = if direction { 8 } else { 0 };
        u64::from_le_bytes(self.0[offset..offset + 8].try_into().unwrap())
    }

    pub fn update_slot(&self) -> u64 {
        u32::from_le_bytes(self.0[16..20].try_into().unwrap()) as u64
    }

    pub fn spread_multiplier(&self) -> u32 {
        u32::from_le_bytes(self.0[20..24].try_into().unwrap())
    }
}

/// Quote result and the market state committed after a swap.
/// `next_quantity` includes input after the previous volume decays.
/// `price_update_slot` is the quote slot, not the oracle update slot.
/// The smoothing adjustments and slot are persisted with the side counter.
pub struct QuoteOutcome {
    pub quote_amount: u64,
    pub next_quantity: u64,
    pub price_update_slot: u64,
    pub smoothed_bid_adjustment: i64,
    pub smoothed_ask_adjustment: i64,
    pub smoothing_slot: u64,
}

impl MarketV2 {
    pub fn quotes_disabled(&self) -> bool {
        bytemuck::bytes_of(self)[Self::QUOTES_DISABLED] != 0
    }

    pub fn set_quotes_disabled(&mut self, disabled: bool) {
        self.0[Self::QUOTES_DISABLED] = u8::from(disabled);
    }
}

impl PricingTiers {
    /// Validates and replaces one pricing tier without changing the others.
    /// The replacement carries all eight extension bytes.
    pub fn update_pricing_tier(
        &mut self,
        index: u8,
        configuration: &PricingTier,
    ) -> Result<(), ProgramError> {
        PricingTier::validate_adjustments(&configuration.bid_adjustments)?;
        PricingTier::validate_adjustments(&configuration.ask_adjustments)?;
        if configuration.staleness_coefficient > 1_000_000 {
            return Err(GoonFiError::InvalidStalenessConfiguration.into());
        }
        if index >= 7 {
            return Err(GoonFiError::InvalidPricingTier.into());
        }
        self.0[index as usize] = *configuration;
        Ok(())
    }
}

/// Pricing tiers and route selectors borrowed from the validated market.
/// The view ends after the two extended class selectors.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PricingView([u8; 0x5da]);

impl PricingView {
    const TIER_STRIDE: usize = core::mem::size_of::<PricingTier>();

    // Invalid selector bytes fall back to the first tier.
    fn tier_index(&self, class: PricingClass) -> u8 {
        let class_offset = match class {
            PricingClass::NoRoute => 0x5aa,
            PricingClass::Unclassified => 0x5ab,
            PricingClass::Titan => 0x5ac,
            PricingClass::JupiterShared => 0x5ad,
            PricingClass::Pro => 0x5ae,
            PricingClass::Authorized => 0x5af,
            PricingClass::Jupiter => 0x5b0,
            PricingClass::PeerMarketMaker => 0x5b1,
            PricingClass::RecognizedSigner => 0x5b2,
            PricingClass::RouterSigner => 0x5b3,
            PricingClass::SecondaryRouter => 0x5b4,
            PricingClass::StandardQuote => 0x5b5,
            PricingClass::TitanQuote => 0x5b6,
            PricingClass::Settler => 0x5b7,
            PricingClass::ProWithAuxiliarySigner => 0x5d8,
            PricingClass::Sun => 0x5d9,
        };
        let tier = self.0[class_offset];
        if tier > 6 { 0u8 } else { tier }
    }

    // The coefficient follows the tier's twenty adjustment words.
    fn coefficient_value(&self, class: PricingClass) -> u64 {
        let tier = self.tier_index(class);
        let offset = tier as usize * Self::TIER_STRIDE
            + PricingTier::STALENESS_COEFFICIENT_OFFSET;
        u64::from_le_bytes(self.0[offset..offset + 8].try_into().unwrap())
    }

    /// Reads the selected tier's per-slot adjustment coefficient.
    pub fn staleness_coefficient(&self, class: PricingClass) -> u64 {
        self.coefficient_value(class)
    }
}

pub(crate) mod creation;
pub use creation::create_market;
pub(crate) use creation::{apply_market_configuration, MarketConfigurationInput};

/// Byte-aligned market view used to calculate quote prices.
///
/// Instances borrow the validated market body from account data.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct MarketPricingView([u8; Self::MINIMUM_LENGTH]);

impl MarketPricingView {
    const MINIMUM_LENGTH: usize = core::mem::size_of::<MarketAccountBody>();
    const PRICING_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, pricing_tiers);
    const BID_QUANTITY_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, bid_quantity);
    const ASK_QUANTITY_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, ask_quantity);
    const BID_UPDATE_SLOT_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, bid_update_slot);
    const ASK_UPDATE_SLOT_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, ask_update_slot);
    const MINIMUM_QUOTE_THRESHOLD_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, minimum_quote_threshold);
    const INVENTORY_KNEE_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, inventory_knee);
    const INVENTORY_LIMIT_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, inventory_limit);
    const NEAR_BID_DENOMINATOR_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, near_bid_denominator);
    const NEAR_ASK_DENOMINATOR_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, near_ask_denominator);
    const FAR_BID_DENOMINATOR_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, far_bid_denominator);
    const FAR_ASK_DENOMINATOR_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, far_ask_denominator);
    const MAXIMUM_QUOTE_AGE_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, maximum_quote_age);
    const GRACE_SLOTS_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, grace_slots);
    const SMOOTHING_PERIOD_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, smoothing_period);
    const SMOOTHING_INTERVAL_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, smoothing_interval);
    const SMOOTHED_BID_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, smoothed_bid_adjustment);
    const SMOOTHED_ASK_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, smoothed_ask_adjustment);
    const SMOOTHING_SLOT_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, smoothing_slot);
    const SMOOTHING_MODE_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, operational_state.smoothing_mode);
    const STALENESS_DISABLED_OFFSET: usize =
        core::mem::offset_of!(MarketAccountBody, operational_state.disable_staleness_adjustment);
    const QUOTE_FLAGS_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, operational_state.quote_flags);
    const LAST_RESTART_SLOT_OFFSET: usize =
        core::mem::offset_of!(MarketAccountBody, operational_state.last_restart_slot);
    const BID_DECIMALS_OFFSET: usize = core::mem::offset_of!(MarketAccountBody, bid_decimals);

    // Invalid selector bytes fall back to the first tier.
    fn tier_index(&self, class: PricingClass) -> u8 {
        let class_offset = match class {
            PricingClass::NoRoute => 0x692,
            PricingClass::Unclassified => 0x693,
            PricingClass::Titan => 0x694,
            PricingClass::JupiterShared => 0x695,
            PricingClass::Pro => 0x696,
            PricingClass::Authorized => 0x697,
            PricingClass::Jupiter => 0x698,
            PricingClass::PeerMarketMaker => 0x699,
            PricingClass::RecognizedSigner => 0x69a,
            PricingClass::RouterSigner => 0x69b,
            PricingClass::SecondaryRouter => 0x69c,
            PricingClass::StandardQuote => 0x69d,
            PricingClass::TitanQuote => 0x69e,
            PricingClass::Settler => 0x69f,
            PricingClass::ProWithAuxiliarySigner => 0x6c0,
            PricingClass::Sun => 0x6c1,
        };
        let tier = self.0[class_offset];
        if tier > 6 { 0 } else { tier }
    }

    /// Validates that the complete market body is present.
    pub fn from_account_data(data: &[u8]) -> Result<&Self, ProgramError> {
        if data.len() < Self::MINIMUM_LENGTH {
            return Err(ProgramError::InvalidAccountData);
        }
        // The view covers the complete validated market body.
        Ok(bytemuck::from_bytes(&data[..Self::MINIMUM_LENGTH]))
    }

    fn pricing(&self) -> &PricingView {
        let start = Self::PRICING_OFFSET;
        let end = start + core::mem::size_of::<PricingView>();
        bytemuck::from_bytes(&self.0[start..end])
    }

    /// Applies the selected tier's adjustment after the grace period.
    pub fn calculate_staleness_adjustment(
        &self,
        current_slot: u64,
        price_update_slot: u64,
        class: PricingClass,
    ) -> Result<u64, ProgramError> {
        if self.0[Self::STALENESS_DISABLED_OFFSET] != 0 {
            return Ok(0);
        }

        let grace_slots = self.0[Self::GRACE_SLOTS_OFFSET] as u64;
        let slots_elapsed =
            current_slot.saturating_sub(price_update_slot.saturating_add(grace_slots));
        let coefficient = self.pricing().coefficient_value(class);
        let wide_adjustment = (slots_elapsed as u128) * (coefficient as u128);
        let adjustment = match u64::try_from(wide_adjustment) {
            Ok(adjustment) => adjustment,
            Err(_) => return staleness_overflow(),
        };

        let adjustment = if current_slot == price_update_slot {
            match adjustment.checked_add(coefficient) {
                Some(value) => value,
                None => return staleness_overflow(),
            }
        } else {
            adjustment
        };
        Ok(adjustment)
    }
}

impl MarketPricingView {
    /// Computes the signed, two-segment inventory adjustment in millionths.
    pub fn inventory_adjustment(&self, direction: bool, deviation: i64) -> Option<i64> {
        let knee = i64::try_from(super::read_u64(&self.0, Self::INVENTORY_KNEE_OFFSET)).ok()?;
        let near_bid = i64::try_from(super::read_u64(
            &self.0,
            Self::NEAR_BID_DENOMINATOR_OFFSET,
        ))
        .ok()?;
        let near_ask = i64::try_from(super::read_u64(
            &self.0,
            Self::NEAR_ASK_DENOMINATOR_OFFSET,
        ))
        .ok()?;
        let far_bid = i64::try_from(super::read_u64(&self.0, Self::FAR_BID_DENOMINATOR_OFFSET))
            .ok()?;
        let far_ask = i64::try_from(super::read_u64(&self.0, Self::FAR_ASK_DENOMINATOR_OFFSET))
            .ok()?;

        let adjustment = if deviation > 0 {
            let near = if direction { near_ask } else { near_bid };
            let far = if direction { far_ask } else { far_bid };
            let near_adjustment =
                i64::try_from(rounded_inventory_ratio(deviation.min(knee), near)?).ok()?;
            let far_adjustment =
                i64::try_from(rounded_inventory_ratio((deviation - knee).max(0), far)?).ok()?;
            near_adjustment.checked_add(far_adjustment)?.checked_neg()?
        } else {
            let quantity = deviation.checked_neg()?;
            let near = if direction { near_bid } else { near_ask };
            let far = if direction { far_bid } else { far_ask };
            let near_adjustment =
                i64::try_from(rounded_inventory_ratio(quantity.min(knee), near)?).ok()?;
            let far_adjustment =
                i64::try_from(rounded_inventory_ratio((quantity - knee).max(0), far)?).ok()?;
            near_adjustment.checked_add(far_adjustment)?
        };
        Some(adjustment)
    }
}

/// Applies an inventory adjustment scaled in millionths.
fn adjusted_inventory_price(raw_price: u64, adjustment: i64) -> Result<u128, ProgramError> {
    let factor = match adjustment.checked_add(1_000_000) {
        Some(factor) if factor > 0 => factor,
        _ => return Err(GoonFiError::InvalidInventoryAdjustment.into()),
    };
    Ok((raw_price as u128 * factor as u128) / 1_000_000)
}

/// Rounds a nonnegative inventory ratio to the nearest millionth.
/// Returns None when the denominator is zero.
fn rounded_inventory_ratio(quantity: i64, denominator: i64) -> Option<u128> {
    if denominator == 0 {
        return None;
    }
    let numerator = (quantity as u128) * 1_000_000
        + ((denominator as u64 / 2) as u128);
    Some(numerator / denominator as u128)
}

fn smoothing_step(decay: u128, weight: u128, exponent: u64) -> (u128, u128, u64) {
    let weight = if exponent & 1 == 1 {
        decay * weight / 1_000_000
    } else {
        weight
    };
    let decay = if exponent > 1 {
        decay * decay / 1_000_000
    } else {
        decay
    };
    (decay, weight, exponent >> 1)
}

/// Advances the inventory adjustment using fixed-point exponential smoothing.
pub fn smooth_inventory_adjustment(
    previous: i64,
    current: i64,
    period: u64,
    elapsed_slots: u64,
) -> Option<i64> {
    let period_minus_one = core::num::NonZeroU64::try_from(period)
        .map(|period| period.get() - 1)
        .ok()?;
    if elapsed_slots == 0 {
        return Some(previous);
    }
    let denominator = period.checked_add(1)?;
    let mut decay = (period_minus_one as u128 * 1_000_000 / denominator as u128).min(1_000_000);
    let mut weight = 1_000_000u128;
    let mut exponent = elapsed_slots;
    while exponent != 0 {
        (decay, weight, exponent) = smoothing_step(decay, weight, exponent);
    }

    // The weights sum to 1_000_000, so the signed blend fits in i128.
    let current_weight = 1_000_000u128 - weight;
    let weighted_sum = weight as i128 * previous as i128
        + current_weight as i128 * current as i128;
    i64::try_from(weighted_sum / 1_000_000).ok()
}

#[cold]
fn quote_calculation_error() -> ProgramError {
    ProgramError::from(GoonFiError::QuoteCalculationFailed)
}

/// Pricing adjustments for one market class, stored in a 176-byte account record.
///
/// | Bytes | Account value |
/// | --- | --- |
/// | 0..80 | ten bid adjustments in millionths |
/// | 80..160 | ten ask adjustments in millionths |
/// | 160..168 | per-slot staleness coefficient |
/// | 168..176 | replacement extension copied verbatim |
///
/// Adjustment arrays must be nondecreasing and at most 1_000_000.
/// The coefficient must also be at most 1_000_000.
/// Borsh reads the extension as raw bytes before the account copy.
#[derive(Clone, Copy, borsh::BorshDeserialize)]
#[repr(C)]
pub struct PricingTier {
    pub bid_adjustments: [u64; 10],
    pub ask_adjustments: [u64; 10],
    pub staleness_coefficient: u64,
    pub extension: [u8; 8],
}

impl PricingTier {
    const STALENESS_COEFFICIENT_OFFSET: usize = core::mem::offset_of!(Self, staleness_coefficient);

    fn validate_adjustments(adjustments: &[u64; 10]) -> Result<(), ProgramError> {
        let mut previous = 0;
        for &adjustment in adjustments {
            if adjustment < previous || adjustment > 1_000_000 {
                return Err(GoonFiError::InvalidPricingTier.into());
            }
            previous = adjustment;
        }
        Ok(())
    }
}

/// The seven serialized tiers at the start of the market's pricing region.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PricingTiers(pub [PricingTier; 7]);

unsafe impl bytemuck::Zeroable for PricingTiers {} // SAFETY: Every tier field accepts zero.
unsafe impl bytemuck::Pod for PricingTiers {} // SAFETY: The tiers have no padding or invalid bit patterns.
pub use goonfi_common::pricing_class::PricingClass;

impl MarketV2 {
    /// Interprets the market body after its discriminator.
    #[inline]
    pub fn market_body_from_bytes(data: &mut [u8]) -> &mut Self {
        bytemuck::from_bytes_mut(data)
    }

    /// Borrows an initialized market account.
    #[inline(always)]
    pub fn borrow_account(account: &AccountInfo) -> Result<Ref<'_, Self>, ProgramError> {
        if account.data_len() != MARKET_ACCOUNT_LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if !account.is_owned_by(&pubkey::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let market_data = account.try_borrow_data()?;
        if market_data[..8] != MARKET_DISCRIMINATOR_BYTES {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Ref::map(market_data, |data| {
            bytemuck::from_bytes(&data[8..MARKET_ACCOUNT_LEN])
        }))
    }

    /// Mutably borrows an initialized market account.
    #[inline(always)]
    pub fn borrow_account_mut(account: &AccountInfo) -> Result<RefMut<'_, Self>, ProgramError> {
        if account.data_len() != MARKET_ACCOUNT_LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if !account.is_owned_by(&pubkey::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let market_data = account.try_borrow_mut_data()?;
        if market_data[..8] != MARKET_DISCRIMINATOR_BYTES {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(RefMut::map(market_data, |data| {
            bytemuck::from_bytes_mut::<Self>(&mut data[8..MARKET_ACCOUNT_LEN])
        }))
    }

    const MINIMUM_LENGTH: usize = core::mem::size_of::<MarketAccountBody>();
    const SEED: usize = core::mem::offset_of!(MarketAccountBody, seed);
    const BID_QUANTITY: usize = core::mem::offset_of!(MarketAccountBody, bid_quantity);
    const ASK_QUANTITY: usize = core::mem::offset_of!(MarketAccountBody, ask_quantity);
    const BID_UPDATE_SLOT: usize = core::mem::offset_of!(MarketAccountBody, bid_update_slot);
    const ASK_UPDATE_SLOT: usize = core::mem::offset_of!(MarketAccountBody, ask_update_slot);

    // Reserve tolerance, side baselines, and quote limit occupy four words.
    // Both limits are expressed in millionths.
    const RESERVE_TOLERANCE: usize = core::mem::offset_of!(MarketAccountBody, reserve_tolerance);
    const BID_RESERVE_BASELINE: usize = core::mem::offset_of!(MarketAccountBody, bid_reserve_baseline);
    const ASK_RESERVE_BASELINE: usize = core::mem::offset_of!(MarketAccountBody, ask_reserve_baseline);
    const QUOTE_LIMIT: usize = core::mem::offset_of!(MarketAccountBody, quote_limit);

    // Swap handlers commit the three smoothing fields together.
    const SMOOTHED_BID_ADJUSTMENT: usize = core::mem::offset_of!(MarketAccountBody, smoothed_bid_adjustment);
    const SMOOTHED_ASK_ADJUSTMENT: usize = core::mem::offset_of!(MarketAccountBody, smoothed_ask_adjustment);
    const SMOOTHING_SLOT: usize = core::mem::offset_of!(MarketAccountBody, smoothing_slot);

    // Withdrawal and restart guards keep independent markers.
    const LAST_LIMITED_WITHDRAWAL_TIMESTAMP: usize =
        core::mem::offset_of!(MarketAccountBody, operational_state.last_limited_withdrawal_timestamp);
    const LAST_RESTART_SLOT: usize = core::mem::offset_of!(MarketAccountBody, operational_state.last_restart_slot);

    // Vault signing uses the bump; quote paths read decimals and the disabled flag.
    const BUMP: usize = core::mem::offset_of!(MarketAccountBody, bump);
    const BID_DECIMALS: usize = core::mem::offset_of!(MarketAccountBody, bid_decimals);
    const ASK_DECIMALS: usize = core::mem::offset_of!(MarketAccountBody, ask_decimals);
    const QUOTES_DISABLED: usize = core::mem::offset_of!(MarketAccountBody, disabled);


    /// Borrows named fields from this validated market view.
    /// The reference remains tied to the account data borrow.
    pub fn as_body(&self) -> &MarketAccountBody {
        bytemuck::cast_ref(self)
    }

    /// Mutably borrows named fields from this validated market view.
    pub fn as_body_mut(&mut self) -> &mut MarketAccountBody {
        bytemuck::cast_mut(self)
    }

    // Integer reads decode little-endian account values.
    // Mutable word borrows update aligned account data.
    fn quantity_at(&self, offset: usize) -> u64 {
        u64::from_le_bytes(self.0[offset..offset + 8].try_into().unwrap())
    }

    fn quantity_at_mut(&mut self, offset: usize) -> &mut u64 {
        bytemuck::from_bytes_mut(&mut self.0[offset..offset + 8])
    }

    fn adjustment_at_mut(&mut self, offset: usize) -> &mut i64 {
        bytemuck::from_bytes_mut(&mut self.0[offset..offset + 8])
    }

    // The market PDA uses this seed with the mint keys and bump.
    pub fn seed(&self) -> [u8; 8] {
        self.0[Self::SEED..Self::SEED + 8].try_into().unwrap()
    }

    // Returned mint keys remain tied to the market account borrow.
    pub fn bid_mint_key(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().bid_mint
    }

    pub fn ask_mint_key(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().ask_mint
    }

    // Vault and oracle keys bind the supplied accounts to this market.
    pub fn bid_vault_key(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().bid_vault
    }

    pub fn ask_vault_key(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().ask_vault
    }

    pub fn oracle_key(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().oracle
    }

    // Program keys select legacy Token versus Token-2022 account handling.
    pub fn bid_token_program(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().bid_token_program
    }

    pub fn ask_token_program(&self) -> &crate::pubkey::Pubkey {
        &self.as_body().ask_token_program
    }

    /// Maximum adverse price change from the reserve baseline, in millionths.
    pub fn reserve_tolerance(&self) -> u64 {
        self.quantity_at(Self::RESERVE_TOLERANCE)
    }

    /// Maximum execution price deviation from the oracle, in millionths.
    pub fn quote_limit(&self) -> u64 {
        self.quantity_at(Self::QUOTE_LIMIT)
    }

    // Mint decimals set the scale for swaps and liquidity checks.
    pub fn bid_decimals(&self) -> u8 {
        self.0[Self::BID_DECIMALS]
    }

    pub fn ask_decimals(&self) -> u8 {
        self.0[Self::ASK_DECIMALS]
    }

    // Swaps mutate only the traded side's cumulative quantity.
    pub fn bid_quantity_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::BID_QUANTITY)
    }

    pub fn ask_quantity_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::ASK_QUANTITY)
    }

    /// Initializes a newly allocated market account.
    #[inline(always)]
    pub fn initialize_account(market_contents: &mut [u8]) -> &mut Self {
        market_contents[..8]
            .copy_from_slice(&MARKET_DISCRIMINATOR_BYTES);
        Self::market_body_from_bytes(&mut market_contents[8..MARKET_ACCOUNT_LEN])
    }

    pub fn bump(&self) -> u8 {
        self.0[Self::BUMP]
    }

    pub fn bid_update_slot_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::BID_UPDATE_SLOT)
    }

    pub fn ask_update_slot_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::ASK_UPDATE_SLOT)
    }

    pub fn bid_reserve_baseline(&self) -> u64 {
        u64::from_le(*self.bid_reserve_baseline_ref())
    }

    pub fn ask_reserve_baseline(&self) -> u64 {
        u64::from_le(*self.ask_reserve_baseline_ref())
    }

    pub fn bid_reserve_baseline_ref(&self) -> &u64 {
        &self.as_body().bid_reserve_baseline
    }

    pub fn ask_reserve_baseline_ref(&self) -> &u64 {
        &self.as_body().ask_reserve_baseline
    }

    pub fn bid_reserve_baseline_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::BID_RESERVE_BASELINE)
    }

    pub fn ask_reserve_baseline_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::ASK_RESERVE_BASELINE)
    }

    pub fn smoothed_bid_adjustment_mut(&mut self) -> &mut i64 {
        self.adjustment_at_mut(Self::SMOOTHED_BID_ADJUSTMENT)
    }

    pub fn smoothed_ask_adjustment_mut(&mut self) -> &mut i64 {
        self.adjustment_at_mut(Self::SMOOTHED_ASK_ADJUSTMENT)
    }

    pub fn smoothing_slot_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::SMOOTHING_SLOT)
    }

    pub fn last_restart_slot_mut(&mut self) -> &mut u64 {
        self.quantity_at_mut(Self::LAST_RESTART_SLOT)
    }

    pub fn last_limited_withdrawal_timestamp(&self) -> i64 {
        let offset = Self::LAST_LIMITED_WITHDRAWAL_TIMESTAMP;
        i64::from_le_bytes(self.0[offset..offset + 8].try_into().unwrap())
    }

    pub fn last_limited_withdrawal_timestamp_mut(&mut self) -> &mut i64 {
        let offset = Self::LAST_LIMITED_WITHDRAWAL_TIMESTAMP;
        bytemuck::from_bytes_mut(&mut self.0[offset..offset + 8])
    }

    pub fn pricing_tiers_mut(&mut self) -> &mut PricingTiers {
        let offset = MarketPricingView::PRICING_OFFSET;
        let length = core::mem::size_of::<PricingTiers>();
        bytemuck::from_bytes_mut(&mut self.0[offset..offset + length])
    }
}

/// Looks up the cumulative quote price, interpolating between active tiers.
/// Zero thresholds and zero adjustments disable a level. The first and
/// last active levels extend to the edges of the quantity range.
pub fn tiered_quote_price(
    market: &MarketPricingView,
    quantity: u64,
    adjusted_price: u128,
    staleness_adjustment: u64,
    side_sign: i128,
    direction: bool,
    class: PricingClass,
    spread_multiplier: u32,
) -> Result<u64, ProgramError> {
    let tier = market.tier_index(class);
    let thresholds_offset = if direction { 0x608 } else { 0x5b8 };
    let adjustments_offset = MarketPricingView::PRICING_OFFSET
        + tier as usize * PricingView::TIER_STRIDE
        + if direction { 80 } else { 0 };
    let price_numerator_at = |adjustment: u64| -> Result<i128, ProgramError> {
        let spread = adjustment
            .checked_add(staleness_adjustment)
            .ok_or_else(invalid_tier_price)?;
        let signed_spread = (spread as i128).checked_mul(side_sign)
            .ok_or_else(invalid_tier_price)?;
        let scaled_spread = signed_spread
            .checked_mul(spread_multiplier as i128)
            .ok_or_else(invalid_tier_price)?;
        let factor = 1_000_000 + scaled_spread / 1_000_000;
        if factor <= 0 {
            return Err(invalid_tier_price());
        }
        let price = i128::try_from(adjusted_price).map_err(|_| invalid_tier_price())?;
        let numerator = checked_signed_quote_product(price, factor)
            .ok_or_else(invalid_tier_price)?;
        Ok(numerator)
    };
    let adjustments: &[[u8; 8]; 10] =
        bytemuck::from_bytes(&market.0[adjustments_offset..adjustments_offset + 80]);
    let thresholds: &[[u8; 8]; 10] =
        bytemuck::from_bytes(&market.0[thresholds_offset..thresholds_offset + 80]);
    #[derive(Clone)]
    struct PreviousTier {
        threshold: u64,
        adjustment: u64,
    }
    let mut previous: Option<PreviousTier> = None;
    for index in 0..10 {
        let adjustment = u64::from_le_bytes(adjustments[index]);
        if pinocchio::hint::unlikely(adjustment == 0) {
            continue;
        }
        let threshold = u64::from_le_bytes(thresholds[index]);
        if threshold == 0 {
            continue;
        }
        let prior = previous.clone();
        previous = Some(PreviousTier {
            threshold,
            adjustment,
        });
        if threshold >= quantity {
            let price = match price_numerator_at(adjustment) {
                Ok(numerator) => u64::try_from(numerator / 1_000_000)
                    .map_err(|_| invalid_tier_price()),
                Err(error) => Err(error),
            }?;
            let priced_prior = match prior {
                Some(PreviousTier {
                    threshold: previous_threshold,
                    adjustment: previous_adjustment,
                }) => {
                    let previous_price =
                        u64::try_from(price_numerator_at(previous_adjustment)? / 1_000_000)
                            .map_err(|_| invalid_tier_price())?;
                    Some((previous_threshold, previous_price))
                }
                None => None,
            };
            let interpolated = priced_prior.map(|(previous_threshold, previous_price)|
                -> Result<u64, ProgramError> {
                let position = quantity.checked_sub(previous_threshold);
                let width = threshold.checked_sub(previous_threshold)
                    .ok_or_else(invalid_tier_price)?;
                if width != 0 {
                    let position = position.ok_or_else(invalid_tier_price)?;
                    let weighted_price = (price as u128) * position as u128;
                    // The selected threshold is at least quantity, so position <= width.
                    let remaining_width = width.wrapping_sub(position);
                    let previous_weighted_price =
                        (previous_price as u128) * remaining_width as u128;
                    let numerator = weighted_price.checked_add(previous_weighted_price)
                        .ok_or_else(invalid_tier_price)?;
                    let numerator = if direction {
                        numerator
                            .checked_add(width as u128)
                            .ok_or_else(invalid_tier_price)?
                            .wrapping_sub(1)
                    } else {
                        numerator
                    };
                    let interpolated_price = u64::try_from(numerator / width as u128)
                        .map_err(|_| invalid_tier_price());
                    interpolated_price
                } else {
                    Ok(price)
                }
            }).transpose()?;
            return Ok(interpolated.unwrap_or(price));
        }
    }
    let adjustment = previous
        .ok_or(ProgramError::from(GoonFiError::InvalidPricingTier))?
        .adjustment;
    u64::try_from(price_numerator_at(adjustment)? / 1_000_000).map_err(|_| invalid_tier_price())
}

fn invalid_tier_price() -> ProgramError {
    ProgramError::from(GoonFiError::TierPriceCalculationFailed)
}

/// Returns the signed pricing product only when it fits in the wide domain.
fn checked_signed_quote_product(left: i128, right: i128) -> Option<i128> {
    left.checked_mul(right)
}

impl MarketPricingView {
    /// Computes the incremental output between two cumulative trade quantities.
    ///
    /// Direct quotes scale quantity by price; inverse quotes divide by price.
    /// Rounding occurs after subtracting the two exact cumulative ratios.
    pub fn incremental_quote_amount(
        &self,
        prior_quantity: u64,
        input_quantity: u64,
        adjusted_price: u128,
        staleness_adjustment: u64,
        side_sign: i128,
        direction: bool,
        class: PricingClass,
        context: Oracle,
        decimal_scale: u128,
    ) -> Result<u64, ProgramError> {
        if prior_quantity == 0 {
            let quote = || -> Result<u64, ProgramError> {
                let price = tiered_quote_price(
                    self,
                    input_quantity,
                    adjusted_price,
                    staleness_adjustment,
                    side_sign,
                    direction,
                    class,
                    context.widener,
                )? as u128;
                let amount = if direction {
                    let result = (input_quantity as u128)
                        .checked_mul(decimal_scale)
                        .and_then(|numerator| numerator.checked_div(price))
                        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                    u64::try_from(result)
                        .map_err(|_| ProgramError::from(GoonFiError::QuoteCalculationFailed))?
                } else {
                    let numerator = price * input_quantity as u128;
                    let result = numerator
                        .checked_div(decimal_scale)
                        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                    u64::try_from(result)
                        .map_err(|_| ProgramError::from(GoonFiError::QuoteCalculationFailed))?
                };
                Ok(amount)
            };
            return quote();
        }
        let cumulative_price_at = |quantity| {
            tiered_quote_price(
                self,
                quantity,
                adjusted_price,
                staleness_adjustment,
                side_sign,
                direction,
                class,
                context.widener,
            )
        };
        let final_quantity = prior_quantity
            .checked_add(input_quantity)
            .ok_or(ProgramError::from(GoonFiError::TradeQuantityOverflow))?;
        let prior_price = cumulative_price_at(prior_quantity)? as u128;
        let final_price = cumulative_price_at(final_quantity)? as u128;
        let is_direct_quote = !direction;
        let result = match is_direct_quote {
            true => {
                if decimal_scale == 0 {
                    Err(quote_calculation_error())
                } else {
                    let prior_numerator = prior_price * prior_quantity as u128;
                    let final_numerator = final_price * final_quantity as u128;
                    let final_quotient = final_numerator / decimal_scale;
                    let prior_quotient = prior_numerator / decimal_scale;
                    let mut difference = final_quotient
                        .checked_sub(prior_quotient)
                        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                    let prior_remainder = prior_numerator % decimal_scale;
                    let final_remainder = final_numerator % decimal_scale;
                    if final_remainder < prior_remainder {
                        difference = difference
                            .checked_sub(1)
                            .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                    }
                    Ok(difference)
                }?
            }
            false => {
                let final_numerator = (final_quantity as u128)
                    .checked_mul(decimal_scale)
                    .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                let prior_numerator = (prior_quantity as u128)
                    .checked_mul(decimal_scale)
                    .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
                floor_ratio_difference(
                    final_numerator,
                    final_price,
                    prior_numerator,
                    prior_price,
                )?
            }
        };
        if result <= u64::MAX as u128 {
            Ok(result as u64)
        } else {
            Err(GoonFiError::QuoteCalculationFailed.into())
        }
    }
}

/// Floors the difference of two nonnegative ratios without widening to 256 bits.
///
/// Rejects zero denominators, a negative result, and cross products that
/// cannot be represented by the pricing arithmetic.
pub fn floor_ratio_difference(
    left_numerator: u128,
    left_denominator: u128,
    right_numerator: u128,
    right_denominator: u128,
) -> Result<u128, ProgramError> {
    let left_quotient = left_numerator
        .checked_div(left_denominator)
        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
    let right_quotient = right_numerator
        .checked_div(right_denominator)
        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
    let left_remainder = left_numerator % left_denominator;
    let right_remainder = right_numerator % right_denominator;
    let difference = left_quotient
        .checked_sub(right_quotient)
        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;

    let fractional_borrow = if left_denominator != right_denominator {
        compare_fractional_products(
            left_remainder,
            right_denominator,
            right_remainder,
            left_denominator,
        )?
    } else {
        left_remainder < right_remainder
    };
    let mut result = difference;
    if fractional_borrow {
        result = result
            .checked_sub(1)
            .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
    }
    Ok(result)
}

fn compare_fractional_products(
    left_remainder: u128,
    right_denominator: u128,
    right_remainder: u128,
    left_denominator: u128,
) -> Result<bool, ProgramError> {
    let left_fraction = left_remainder
        .checked_mul(right_denominator)
        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
    let right_fraction = right_remainder
        .checked_mul(left_denominator)
        .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;
    Ok(left_fraction < right_fraction)
}

impl MarketPricingView {
    /// Calculates a quote and the pricing state to save after execution.
    pub fn calculate_quote_price(
        &self,
        input_quantity: u64,
        minimum_output: u64,
        current_slot: u64,
        direction: bool,
        vault_amount: u64,
        class: PricingClass,
        context: Oracle,
    ) -> Result<QuoteOutcome, ProgramError> {
        prologue_padding!(8, 3);
        let price_update_slot = u64::from(context.slot);
        let stored_restart_slot = super::read_u64(&self.0, Self::LAST_RESTART_SLOT_OFFSET);
        if stored_restart_slot != 0 {
            let last_restart_slot = read_last_restart_slot()?;
            if last_restart_slot > stored_restart_slot {
                return Err(GoonFiError::MarketRestartSlotStale.into());
            }
        }
        if price_update_slot + (self.0[Self::MAXIMUM_QUOTE_AGE_OFFSET] as u64) < current_slot {
            return Err(GoonFiError::StaleOraclePrice.into());
        }
        let balance = i64::try_from(vault_amount)
            .map_err(|_| ProgramError::from(GoonFiError::VaultBalanceOutOfRange))?;
        let inventory_limit_raw = super::read_u64(&self.0, Self::INVENTORY_LIMIT_OFFSET);
        if inventory_limit_raw > i64::MAX as u64 {
            return Err(GoonFiError::InvalidInventoryAdjustment.into());
        }
        let inventory_limit = inventory_limit_raw as i64;
        let deviation = balance - inventory_limit;
        let raw_price = if direction { context.ask_price } else { context.bid_price };
        let smoothing_mode = self.0[Self::SMOOTHING_MODE_OFFSET] != 0;

        let (adjusted_price, smoothed_bid_adjustment, smoothed_ask_adjustment, smoothing_slot) =
            if smoothing_mode {
                let raw_bid = self
                    .inventory_adjustment(true, deviation)
                    .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
                let raw_ask = self
                    .inventory_adjustment(false, deviation)
                    .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
                let previous_slot = super::read_u64(&self.0, Self::SMOOTHING_SLOT_OFFSET);
                let (bid, ask) = if previous_slot != 0 {
                    let elapsed_slots = current_slot.saturating_sub(previous_slot);
                    let previous_bid =
                        super::read_u64(&self.0, Self::SMOOTHED_BID_OFFSET) as i64;
                    let period = super::read_u64(&self.0, Self::SMOOTHING_PERIOD_OFFSET);
                    let previous_ask =
                        super::read_u64(&self.0, Self::SMOOTHED_ASK_OFFSET) as i64;
                    let bid =
                        smooth_inventory_adjustment(previous_bid, raw_bid, period, elapsed_slots)
                            .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
                    let ask =
                        smooth_inventory_adjustment(previous_ask, raw_ask, period, elapsed_slots)
                            .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
                    (bid, ask)
                } else {
                    (raw_bid, raw_ask)
                };
                let interval =
                    super::read_u64(&self.0, Self::SMOOTHING_INTERVAL_OFFSET) as u128;
                let adjustment = if direction {
                    let candidate = if (raw_bid as i128 - bid as i128).unsigned_abs() < interval {
                        raw_bid
                    } else {
                        bid
                    };
                    candidate.max(raw_bid)
                } else {
                    let candidate = if (raw_ask as i128 - ask as i128).unsigned_abs() < interval {
                        raw_ask
                    } else {
                        ask
                    };
                    candidate.min(raw_ask)
                };
                let adjusted_price = adjusted_inventory_price(raw_price, adjustment)?;
                (adjusted_price, bid, ask, current_slot)
            } else {
                let adjustment = self
                    .inventory_adjustment(direction, deviation)
                    .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
                let adjusted_price = adjusted_inventory_price(raw_price, adjustment)?;
                (
                    adjusted_price,
                    super::read_u64(&self.0, Self::SMOOTHED_BID_OFFSET) as i64,
                    super::read_u64(&self.0, Self::SMOOTHED_ASK_OFFSET) as i64,
                    super::read_u64(&self.0, Self::SMOOTHING_SLOT_OFFSET),
                )
            };

        let mut staleness =
            self.calculate_staleness_adjustment(current_slot, price_update_slot, class)?;
        let side_sign = if direction { 1i128 } else { -1i128 };
        let decimal_scale =
            10u128.checked_pow(self.0[Self::BID_DECIMALS_OFFSET] as u32)
                .ok_or(ProgramError::from(GoonFiError::QuoteCalculationFailed))?;

        let bid_update_slot = super::read_u64(&self.0, Self::BID_UPDATE_SLOT_OFFSET);
        let ask_update_slot = super::read_u64(&self.0, Self::ASK_UPDATE_SLOT_OFFSET);
        if bid_update_slot.max(ask_update_slot) == current_slot
            && self.0[Self::QUOTE_FLAGS_OFFSET] == 0
        {
            staleness = staleness.max(self.pricing().staleness_coefficient(class));
        }

        let side_offset = if direction {
            Self::ASK_UPDATE_SLOT_OFFSET
        } else {
            Self::BID_UPDATE_SLOT_OFFSET
        };
        // SAFETY: Both offsets point to complete u64 fields in this validated market body.
        let side_update_slot = unsafe {
            self.0.as_ptr().add(side_offset).cast::<u64>().read_unaligned()
        };
        let slots_elapsed = current_slot.saturating_sub(side_update_slot);
        let decay = (slots_elapsed as u128)
            * super::read_u64(&self.0, Self::MINIMUM_QUOTE_THRESHOLD_OFFSET) as u128;
        let decayed_quantity = u64::try_from(decay).unwrap_or(u64::MAX);
        let cumulative_quantity = if direction {
            super::read_u64(&self.0, Self::ASK_QUANTITY_OFFSET)
        } else {
            super::read_u64(&self.0, Self::BID_QUANTITY_OFFSET)
        };
        let prior_quantity = cumulative_quantity.saturating_sub(decayed_quantity);
        let first_quote = self.incremental_quote_amount(
            prior_quantity,
            input_quantity,
            adjusted_price,
            staleness,
            side_sign,
            direction,
            class,
            context,
            decimal_scale,
        )?;

        let mut quote_amount = first_quote;
        if smoothing_mode {
            let previous_adjustment = self
                .inventory_adjustment(direction, deviation)
                .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
            let trade_quantity = if !direction {
                input_quantity
            } else {
                first_quote
            };
            let trade_quantity =
                i64::try_from(trade_quantity).map_err(|_| quote_calculation_error())?;
            let next_deviation = if direction {
                deviation.checked_sub(trade_quantity)
            } else {
                deviation.checked_add(trade_quantity)
            }
            .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
            let next_adjustment = self
                .inventory_adjustment(direction, next_deviation)
                .ok_or(ProgramError::from(GoonFiError::InvalidInventoryAdjustment))?;
            let adjustment_change = next_adjustment.abs_diff(previous_adjustment);
            if adjustment_change != 0 {
                let next_staleness = staleness
                    .checked_add(adjustment_change)
                    .ok_or(ProgramError::from(GoonFiError::StalenessAdjustmentOverflow))?;
                quote_amount = self.incremental_quote_amount(
                    prior_quantity,
                    input_quantity,
                    adjusted_price,
                    next_staleness,
                    side_sign,
                    direction,
                    class,
                    context,
                    decimal_scale,
                )?;
            }
        }

        if quote_amount == 0 {
            return Err(GoonFiError::QuoteCalculationFailed.into());
        }
        if quote_amount < minimum_output {
            return Err(GoonFiError::InsufficientSwapOutput.into());
        }
        let next_quantity = prior_quantity
            .checked_add(input_quantity)
            .ok_or(ProgramError::from(GoonFiError::TradeQuantityOverflow))?;
        Ok(QuoteOutcome {
            quote_amount,
            next_quantity,
            price_update_slot: current_slot,
            smoothed_bid_adjustment,
            smoothed_ask_adjustment,
            smoothing_slot,
        })
    }
}

#[cold]
fn staleness_overflow<T>() -> Result<T, ProgramError> {
    Err(GoonFiError::StalenessAdjustmentOverflow.into())
}

fn read_last_restart_slot() -> Result<u64, ProgramError> {
    let mut value = core::mem::MaybeUninit::<u64>::uninit();
    // SAFETY: The syscall receives a writable buffer large enough for one u64.
    let status = unsafe {
        pinocchio::syscalls::sol_get_last_restart_slot(value.as_mut_ptr().cast::<u8>())
    };
    if status != 0 {
        return Err(ProgramError::UnsupportedSysvar);
    }
    // SAFETY: A successful syscall writes all eight bytes of the restart slot.
    Ok(unsafe { value.assume_init() })
}

/// Seed shared by a market’s bid and ask token vaults.
pub static VAULT_SEED: [u8; 5] = *b"vault";

impl MarketV2 {
    /// Closes an empty market vault using the market PDA as its authority.
    pub(crate) fn close_market_vault(
        &self,
        market: &AccountInfo,
        vault: &AccountInfo,
        destination: &AccountInfo,
        token_program: &crate::pubkey::Pubkey,
    ) -> crate::internal::ProgramResult {
        prologue_padding!(13, 3);
        use crate::{
            extension::{Seed, Signer},
            internal::ProgramError,
            pubkey,
            state::token,
        };

        let vault_amount = goonfi_common::helpers::get_token_account_amount(vault, token_program)?;
        if vault_amount != 0 {
            return Err(GoonFiError::VaultNotEmpty.into());
        }

        let market_seed = self.seed();
        let bump = [self.bump()];
        let seeds = [
            Seed::from(&b"market_v2"[..]),
            Seed::from(self.bid_mint_key()),
            Seed::from(self.ask_mint_key()),
            Seed::from(&market_seed),
            Seed::from(&bump),
        ];
        if token_program == &pubkey::TOKEN_PROGRAM_ID {
            let close = token::CloseAccount {
                account: vault,
                destination,
                authority: market,
            };
            let signers = [Signer::from(&seeds)];
            close.invoke_signed(&signers)
        } else if token_program == &pubkey::TOKEN_2022_PROGRAM_ID {
            let close = pinocchio_token_2022::instructions::CloseAccount {
                account: vault,
                destination,
                authority: market,
                token_program,
            };
            let signers = [Signer::from(&seeds)];
            close.invoke_signed(&signers)
        } else {
            Err(ProgramError::IncorrectProgramId)
        }
    }
}

pub use MarketV2 as MarketConfigurationState;

/// Extended route class selectors; the first two bytes select the auxiliary
/// signer and Sun tiers, and the remaining bytes stay in the account record.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ExtendedClassTierSelectors(pub [u8; 16]);

/// Control bytes between the quote flags and limited-withdrawal timestamp.
/// Market creation initializes these bytes to zero.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct MarketControlExtension(pub [u8; 21]);

/// Account words after the last restart slot.
/// Market creation initializes these words to zero.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct MarketAccountingExtension(pub [u64; 18]);

/// Market flags and lifecycle records following the smoothing state.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MarketOperationalState {
    pub smoothing_mode: u8,
    pub disable_staleness_adjustment: u8,
    pub quote_flags: u8,
    pub control_extension: MarketControlExtension,
    pub last_limited_withdrawal_timestamp: i64,
    pub last_restart_slot: u64,
    pub accounting_extension: MarketAccountingExtension,
}

/// Named fields of the market body following its eight-byte discriminator.
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct MarketAccountBody {
    pub seed: u64,
    pub bid_quantity: u64,
    pub ask_quantity: u64,
    pub bid_update_slot: u64,
    pub ask_update_slot: u64,
    pub authority: Pubkey,
    pub bid_mint: Pubkey,
    pub ask_mint: Pubkey,
    pub bid_vault: Pubkey,
    pub ask_vault: Pubkey,
    pub oracle: Pubkey,
    pub pricing_tiers: [PricingTier; 7],
    pub bid_thresholds: [u64; 10],
    pub ask_thresholds: [u64; 10],
    pub minimum_quote_threshold: u64,
    pub near_bid_denominator: u64,
    pub near_ask_denominator: u64,
    pub inventory_knee: u64,
    pub far_bid_denominator: u64,
    pub far_ask_denominator: u64,
    pub inventory_limit: u64,
    pub maximum_quote_age: u8,
    pub grace_slots: u8,
    pub class_tiers: [u8; 14],
    pub reserve_tolerance: u64,
    pub bid_reserve_baseline: u64,
    pub ask_reserve_baseline: u64,
    pub quote_limit: u64,
    pub extended_class_tiers: ExtendedClassTierSelectors,
    pub smoothing_period: u64,
    pub smoothing_interval: u64,
    pub smoothed_bid_adjustment: i64,
    pub smoothed_ask_adjustment: i64,
    pub smoothing_slot: u64,
    pub operational_state: MarketOperationalState,
    pub bump: u8,
    pub ask_vault_bump: u8,
    pub bid_vault_bump: u8,
    pub unused_vault_bump: u8,
    pub bid_decimals: u8,
    pub ask_decimals: u8,
    pub disabled: u8,
    pub reserved: u8,
    pub bid_token_program: Pubkey,
    pub ask_token_program: Pubkey,
}

// SAFETY: Every field accepts an all-zero bit pattern.
unsafe impl bytemuck::Zeroable for MarketAccountBody {}
// SAFETY: The C layout has no padding and every field accepts any bit pattern.
unsafe impl bytemuck::Pod for MarketAccountBody {}

// SAFETY: This transparent byte-array view accepts an all-zero body.
unsafe impl bytemuck::Zeroable for MarketPricingView {}
// SAFETY: The byte array has no padding or invalid bit patterns.
unsafe impl bytemuck::Pod for MarketPricingView {}

// SAFETY: This transparent pricing-region view accepts all-zero selectors.
unsafe impl bytemuck::Zeroable for PricingView {}
// SAFETY: The byte array has no padding or invalid bit patterns.
unsafe impl bytemuck::Pod for PricingView {}
