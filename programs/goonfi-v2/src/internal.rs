pub mod instruction_sysvar {
    pub use goonfi_account_validation::{classify_signed_route, transaction_has_recognized_signer};
}

/// Solana's invocation heap uses Pinocchio's bump allocator.
pub mod allocator {
    #[global_allocator]
    static ALLOCATOR: pinocchio::entrypoint::BumpAllocator = pinocchio::entrypoint::BumpAllocator {
        start: pinocchio::entrypoint::HEAP_START_ADDRESS as usize,
        len: pinocchio::entrypoint::HEAP_LENGTH,
    };
}

pub mod sysvars {
    use super::ProgramError;
    use pinocchio::sysvars::Sysvar;

    pub use pinocchio::sysvars::clock::Clock;

    /// Obtains the clock supplied by the Solana runtime.
    pub fn get_clock() -> Result<Clock, ProgramError> {
        Clock::get()
    }
}

pub use pinocchio::program_error::ProgramError;
pub type ProgramResult = pinocchio::ProgramResult;

/// Program-specific errors returned by GoonFi instructions and market state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GoonFiError {
    /// Supplied token decimals do not match the market mints.
    MintDecimalsMismatch = 1,
    /// The bid and ask mints have the same address.
    IdenticalMarketMints = 2,
    /// The maximum quote age exceeds 200 slots.
    InvalidMaximumQuoteAge = 4,
    /// Inventory parameters or arithmetic cannot produce a valid adjustment.
    InvalidInventoryAdjustment = 5,
    /// A staleness adjustment cannot fit in a u64.
    StalenessAdjustmentOverflow = 8,
    /// Quote arithmetic fails, exceeds its range, or produces zero output.
    QuoteCalculationFailed = 9,
    /// The vault balance cannot fit signed inventory arithmetic.
    VaultBalanceOutOfRange = 10,
    /// The requested swap amount is zero.
    ZeroSwapAmount = 11,
    /// Tier price calculation cannot produce a valid u64 price.
    TierPriceCalculationFailed = 12,
    /// A tier index, adjustment schedule, or active tier selection is invalid.
    InvalidPricingTier = 13,
    /// The market is disabled for swaps.
    MarketDisabled = 14,
    /// The quoted output is below the requested minimum.
    InsufficientSwapOutput = 15,
    /// Adding the swap input to the prior quantity overflows.
    TradeQuantityOverflow = 16,
    /// A bid inventory denominator exceeds its ask counterpart.
    InvalidInventoryDenominatorOrder = 18,
    /// The inventory limit does not exceed the inventory knee.
    InvalidInventoryRange = 19,
    /// The oracle update is older than the maximum quote age.
    StaleOraclePrice = 21,
    /// A staleness coefficient or grace period exceeds its bound.
    InvalidStalenessConfiguration = 23,
    /// The Bloom filter reports the key present during insertion.
    BloomKeyAlreadyPresent = 24,
    /// The Bloom filter reports the key absent during removal.
    BloomKeyNotPresent = 25,
    /// A token vault still has a balance when closure is requested.
    VaultNotEmpty = 26,
    /// The supplied oracle account differs from the market oracle.
    OracleAccountMismatch = 32,
    /// The threshold sequence or minimum quote threshold is invalid.
    InvalidQuoteThresholds = 33,
    /// A configured token program is neither Token nor Token-2022.
    UnsupportedMarketTokenProgram = 35,
    /// The oracle price moved adversely beyond the reserve tolerance.
    ReservePriceToleranceExceeded = 36,
    /// The bid rate exceeds one million millionths.
    BidRateOutOfRange = 37,
    /// The execution price exceeds its limit or the comparison cannot be computed.
    ExecutionPriceLimitCheckFailed = 38,
    /// The ask rate exceeds one million millionths.
    AskRateOutOfRange = 39,
    /// A route program, route authority, or auxiliary authority is not permitted.
    InvalidRouteAuthorization = 41,
    /// The smoothing mode or its enabled period is invalid.
    InvalidSmoothingConfiguration = 42,
    /// Neither the token account nor its owner is authorized for liquidity.
    UnauthorizedLiquidityAccount = 43,
    /// A whitelist update exceeds capacity or contains duplicate entries.
    InvalidWhitelistUpdate = 44,
    /// A limited bid withdrawal exceeds its allowed amount.
    LimitedWithdrawalAmountTooLarge = 45,
    /// The cooldown since the prior limited withdrawal has not elapsed.
    LimitedWithdrawalCooldown = 46,
    /// Limited withdrawal requires a nonzero native bid amount and zero ask amount.
    InvalidLimitedWithdrawal = 47,
    /// The cluster restart slot is newer than the market's saved slot.
    MarketRestartSlotStale = 48,
}

impl From<GoonFiError> for ProgramError {
    fn from(error: GoonFiError) -> Self {
        Self::Custom(error as u32)
    }
}
