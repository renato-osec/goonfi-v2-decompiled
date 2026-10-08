use crate::{
    account_info::AccountInfo,
    instructions::market_admin::InitializeMarketParameters,
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
    state::market_v2::{MarketConfigurationState, PricingTier, PricingTiers, MARKET_ACCOUNT_LEN},
    utils::helpers::prologue_padding,
};

/// Serialized inputs for a mutable market configuration update.
#[derive(borsh::BorshDeserialize)]
pub(crate) struct MarketConfigurationInput {
    class_0: u8,
    class_1: u8,
    class_2: u8,
    class_3: u8,
    class_4: u8,
    class_5: u8,
    class_6: u8,
    class_7: u8,
    class_8: u8,
    class_9: u8,
    class_10: u8,
    class_11: u8,
    class_12: u8,
    class_13: u8,
    class_14: u8,
    class_15: u8,
    bid_thresholds: [u64; 10],
    ask_thresholds: [u64; 10],
    minimum_quote_threshold: u64,
    near_bid_denominator: u64,
    near_ask_denominator: u64,
    inventory_knee: u64,
    far_bid_denominator: u64,
    far_ask_denominator: u64,
    inventory_limit: u64,
    maximum_quote_age: u8,
    grace_slots: u8,
    bid_token_program: Pubkey,
    ask_token_program: Pubkey,
    bid_rate_millionths: u64,
    ask_rate_millionths: u64,
    smoothing_period: u64,
    smoothing_interval: u64,
    smoothing_mode: u8,
    disable_staleness_adjustment: u8,
    quote_flags: u8,
}

/// Validates the configuration, then creates the market PDA and token vaults.
pub fn create_market(
    payer: &AccountInfo,
    market: &AccountInfo,
    bid_mint: &AccountInfo,
    ask_mint: &AccountInfo,
    bid_vault: &AccountInfo,
    ask_vault: &AccountInfo,
    oracle: &AccountInfo,
    bid_program: &AccountInfo,
    ask_program: &AccountInfo,
    parameters: std::boxed::Box<InitializeMarketParameters>,
) -> ProgramResult {
    use crate::{extension::Seed, state::token, utils::cpi::create_or_allocate_account};

    const TIER_BYTES: usize = core::mem::size_of::<[PricingTier; 7]>();
    const THRESHOLD_BYTES: usize = core::mem::size_of::<[u64; 10]>();
    const BID_THRESHOLDS: usize = TIER_BYTES;
    const ASK_THRESHOLDS: usize = BID_THRESHOLDS + THRESHOLD_BYTES;
    const CURVE_PARAMETERS: usize = ASK_THRESHOLDS + THRESHOLD_BYTES;
    const MAXIMUM_QUOTE_AGE_OFFSET: usize = CURVE_PARAMETERS + 7 * core::mem::size_of::<u64>();
    const CLASS_TIERS_OFFSET: usize = MAXIMUM_QUOTE_AGE_OFFSET + 2;
    const BID_RATE_OFFSET: usize = CLASS_TIERS_OFFSET + 14;
    const ASK_RATE_OFFSET: usize = BID_RATE_OFFSET + 0x18;
    const EXTENDED_CLASS_TIERS_OFFSET: usize = ASK_RATE_OFFSET + core::mem::size_of::<u64>();
    const SMOOTHING_PERIOD_OFFSET: usize = EXTENDED_CLASS_TIERS_OFFSET + 0x10;
    const SMOOTHING_INTERVAL_OFFSET: usize = SMOOTHING_PERIOD_OFFSET + core::mem::size_of::<u64>();
    const FLAGS_OFFSET: usize = SMOOTHING_INTERVAL_OFFSET + 0x20;
    const RESTART_SLOT_OFFSET: usize = FLAGS_OFFSET + 0x20;
    const SETTINGS_BYTES: usize = 0x6c8;
    const MARKET_STATE_WORDS: usize =
        core::mem::size_of::<MarketConfigurationState>() / core::mem::size_of::<u64>();

    fn read_restart_slot() -> Result<u64, ProgramError> {
        let mut restart_slot = core::mem::MaybeUninit::<u64>::uninit();
        // SAFETY: the syscall receives a writable eight-byte output slot.
        let status = unsafe {
            pinocchio::syscalls::sol_get_last_restart_slot(restart_slot.as_mut_ptr().cast::<u8>())
        };
        if status != 0 {
            return Err(ProgramError::UnsupportedSysvar);
        }
        // SAFETY: a successful syscall initializes the output slot.
        Ok(unsafe { restart_slot.assume_init() })
    }

    // Leading zero thresholds may precede a strictly increasing nonzero suffix.
    #[inline(always)]
    fn validate_thresholds(thresholds: &[u64; 10]) -> ProgramResult {
        struct SequenceState {
            seen_nonzero: bool,
            previous: u64,
            saw_zero_after_nonzero: bool,
        }
        let mut state = SequenceState {
            seen_nonzero: false,
            previous: 0,
            saw_zero_after_nonzero: false,
        };

        for &threshold in thresholds.iter() {
            if threshold == 0 {
                if state.seen_nonzero {
                    state.saw_zero_after_nonzero = true;
                }
                continue;
            }

            if state.saw_zero_after_nonzero {
                return Err(GoonFiError::InvalidQuoteThresholds.into());
            }
            if threshold <= state.previous {
                return Err(GoonFiError::InvalidQuoteThresholds.into());
            }
            state.previous = threshold;
            state.seen_nonzero = true;
        }

        if state.saw_zero_after_nonzero {
            return Err(GoonFiError::InvalidQuoteThresholds.into());
        }

        Ok(())
    }

    #[inline(always)]
    fn decode_token2022_decimals(mint: &AccountInfo) -> Result<u8, ProgramError> {
        let data = mint.try_borrow_data()?;
        let state =
            spl_token_2022::extension::StateWithExtensions::<spl_token_2022::state::Mint>::unpack(
                &data,
            )
            .map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(state.base.decimals)
    }

    #[inline(always)]
    fn get_legacy_bid_decimals(mint: &AccountInfo) -> Result<u8, ProgramError> {
        let mint =
            token::Mint::from_account_info(mint).map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(mint.decimals())
    }

    // Selects the mint decoder for the supplied ask-side token program.
    #[inline(always)]
    fn get_ask_decimals(mint: &AccountInfo, program: &AccountInfo) -> Result<u8, ProgramError> {
        if program.key() == &pubkey::TOKEN_PROGRAM_ID {
            Ok(token::Mint::from_account_info(mint)
                .map_err(|_| ProgramError::InvalidAccountData)?
                .decimals())
        } else if program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
            decode_token2022_decimals(mint)
        } else {
            Err(ProgramError::IncorrectProgramId)
        }
    }

    struct MarketAccounts<'a> {
        payer: &'a AccountInfo,
        market: &'a AccountInfo,
        bid_mint: &'a AccountInfo,
        ask_mint: &'a AccountInfo,
        bid_vault: &'a AccountInfo,
        ask_vault: &'a AccountInfo,
        oracle: &'a AccountInfo,
        bid_program: &'a AccountInfo,
        ask_program: &'a AccountInfo,
    }

    // Validates staged settings, then initializes the market and its vaults.
    #[inline(always)]
    fn finish_create_market(
        accounts: MarketAccounts<'_>,
        parameters: std::boxed::Box<InitializeMarketParameters>,
        initial_tiers: [PricingTier; 7],
        mut settings_words: core::mem::MaybeUninit<[u64; SETTINGS_BYTES / 8]>,
        class_tiers: [u8; 16],
    ) -> ProgramResult {
        let MarketAccounts {
            payer,
            market,
            bid_mint,
            ask_mint,
            bid_vault,
            ask_vault,
            oracle,
            bid_program,
            ask_program,
        } = accounts;
        let minimum_quote_threshold = parameters.minimum_quote_threshold;
        let near_bid_denominator = parameters.near_bid_denominator;
        let near_ask_denominator = parameters.near_ask_denominator;
        let inventory_knee = parameters.inventory_knee;
        let far_bid_denominator = parameters.far_bid_denominator;
        let far_ask_denominator = parameters.far_ask_denominator;
        let inventory_limit = parameters.inventory_limit;
        let maximum_quote_age = parameters.maximum_quote_age;
        let grace_slots = parameters.grace_slots;
        let bid_rate_millionths = parameters.bid_rate_millionths;
        let ask_rate_millionths = parameters.ask_rate_millionths;
        let smoothing_period = parameters.smoothing_period;
        let smoothing_interval = parameters.smoothing_interval;
        let smoothing_mode = parameters.smoothing_mode;
        let disable_staleness_adjustment = parameters.disable_staleness_adjustment;
        let quote_flags = parameters.quote_flags;
        let settings = settings_words.as_mut_ptr().cast::<u8>();
        let restart_slot = read_restart_slot()?;
        // Zero the reserved spans that the sequential writer will skip.
        // SAFETY: both spans lie within the writable 1,736-byte settings buffer.
        unsafe {
            core::ptr::write_bytes(settings.add(RESTART_SLOT_OFFSET + 8), 0, 0x10);
            core::ptr::write_bytes(settings.add(SETTINGS_BYTES - 0x20), 0, 0x20);
        }
        // The writer appends settings fields in account layout order.
        struct SettingsWriter<'a> {
            words: &'a mut core::mem::MaybeUninit<[u64; SETTINGS_BYTES / 8]>,
            cursor: usize,
        }
        impl SettingsWriter<'_> {
            fn word(&mut self, value: u64) {
                let destination = self
                    .words
                    .as_mut_ptr()
                    .cast::<u8>()
                    .wrapping_add(self.cursor);
                // SAFETY: the buffer and word cursor are aligned, with eight bytes in bounds.
                unsafe { destination.cast::<u64>().write(value) };
                self.cursor += 8;
            }
            fn byte(&mut self, value: u8) {
                let destination = self
                    .words
                    .as_mut_ptr()
                    .cast::<u8>()
                    .wrapping_add(self.cursor);
                // SAFETY: each byte cursor stays within the writable settings buffer.
                unsafe { destination.write(value) };
                self.cursor += 1;
            }
            fn zeros(&mut self, len: usize) {
                let destination = self
                    .words
                    .as_mut_ptr()
                    .cast::<u8>()
                    .wrapping_add(self.cursor);
                // SAFETY: each zeroed span stays within the writable settings buffer.
                unsafe { core::ptr::write_bytes(destination, 0, len) };
                self.cursor += len;
            }
            fn skip_zeroed(&mut self, len: usize) {
                self.cursor += len;
            }
            fn align_to(&mut self, alignment: usize) {
                let skip = self
                    .words
                    .as_mut_ptr()
                    .cast::<u8>()
                    .wrapping_add(self.cursor)
                    .align_offset(alignment);
                self.cursor += skip;
            }
        }
        let mut writer = SettingsWriter {
            words: &mut settings_words,
            cursor: 0,
        };
        let destination = writer
            .words
            .as_mut_ptr()
            .cast::<u8>()
            .wrapping_add(writer.cursor);
        // SAFETY: the tier array and destination prefix are disjoint and equally sized.
        unsafe {
            core::ptr::copy_nonoverlapping(
                initial_tiers.as_ptr().cast::<u8>(),
                destination,
                TIER_BYTES,
            )
        };
        writer.cursor += TIER_BYTES;
        let destination = writer
            .words
            .as_mut_ptr()
            .cast::<u8>()
            .wrapping_add(writer.cursor);
        // SAFETY: the bid thresholds and their destination are disjoint 80-byte spans.
        unsafe {
            core::ptr::copy_nonoverlapping(
                parameters.bid_thresholds.as_ptr().cast::<u8>(),
                destination,
                THRESHOLD_BYTES,
            )
        };
        writer.cursor += THRESHOLD_BYTES;
        let destination = writer
            .words
            .as_mut_ptr()
            .cast::<u8>()
            .wrapping_add(writer.cursor);
        // SAFETY: the ask thresholds and their destination are disjoint 80-byte spans.
        unsafe {
            core::ptr::copy_nonoverlapping(
                parameters.ask_thresholds.as_ptr().cast::<u8>(),
                destination,
                THRESHOLD_BYTES,
            )
        };
        writer.cursor += THRESHOLD_BYTES;
        writer.word(minimum_quote_threshold);
        writer.word(near_bid_denominator);
        writer.word(near_ask_denominator);
        writer.word(inventory_knee);
        writer.word(far_bid_denominator);
        writer.word(far_ask_denominator);
        writer.word(inventory_limit);
        writer.byte(maximum_quote_age);
        writer.byte(grace_slots.min(50));
        // Append the base class selectors before the two extended selectors.
        class_tiers[..14]
            .iter()
            .copied()
            .for_each(|class| writer.byte(class));
        // Align the following rate field.
        writer.align_to(8);
        writer.word(bid_rate_millionths);
        writer.zeros(0x10);
        writer.word(ask_rate_millionths);
        writer.byte(class_tiers[14]);
        writer.byte(class_tiers[15]);
        writer.zeros(14);
        writer.word(smoothing_period);
        writer.word(smoothing_interval);
        writer.zeros(0x18);
        writer.byte(smoothing_mode);
        writer.byte(disable_staleness_adjustment);
        writer.byte(quote_flags);
        writer.zeros(0x1d);
        writer.word(restart_slot);
        writer.skip_zeroed(0x10);
        writer.zeros(0x60);
        writer.skip_zeroed(0x20);
        // SAFETY: the writer filled every byte of the u64 array, including reserved spans.
        let settings_words = unsafe { settings_words.assume_init_ref() };

        // Check the staged settings before allocating any accounts.
        let validate_configuration = || -> ProgramResult {
            if near_bid_denominator > near_ask_denominator
                || far_bid_denominator > far_ask_denominator
            {
                return Err(GoonFiError::InvalidInventoryDenominatorOrder.into());
            }
            if inventory_limit <= inventory_knee {
                return Err(GoonFiError::InvalidInventoryRange.into());
            }
            if maximum_quote_age > 200 {
                return Err(GoonFiError::InvalidMaximumQuoteAge.into());
            }

            let tiers: &PricingTiers =
                bytemuck::from_bytes(&bytemuck::bytes_of(settings_words)[..TIER_BYTES]);
            // Check each tier's adjustment schedules and staleness factor.
            let validate_initial_tiers = || -> ProgramResult {
                for tier in &tiers.0 {
                    // Apply the same adjustment rule to bid and ask sides.
                    let valid = (0..2).all(|side| {
                        let adjustments = if side == 0 {
                            &tier.bid_adjustments
                        } else {
                            &tier.ask_adjustments
                        };
                        let mut previous = 0;
                        for &value in adjustments {
                            if value < previous || value > 1_000_000 {
                                return false;
                            }
                            previous = value;
                        }
                        true
                    });
                    if !valid {
                        return Err(GoonFiError::InvalidPricingTier.into());
                    }
                    // Zero disables staleness; active coefficients keep the price factor within 2x.
                    let coefficient = tier.staleness_coefficient;
                    if coefficient != 0
                        && coefficient
                            .checked_add(1_000_000)
                            .is_none_or(|factor| factor > 2_000_000)
                    {
                        return Err(GoonFiError::InvalidStalenessConfiguration.into());
                    }
                }
                Ok(())
            };
            validate_initial_tiers()?;

            let threshold_words: &[u64; 20] = settings_words
                [BID_THRESHOLDS / 8..CURVE_PARAMETERS / 8]
                .try_into()
                .unwrap();
            let threshold_groups: &[[u64; 10]; 2] = bytemuck::cast_ref(threshold_words);
            let bid_thresholds = &threshold_groups[0];
            let ask_thresholds = &threshold_groups[1];
            validate_thresholds(bid_thresholds)?;
            validate_thresholds(ask_thresholds)?;
            let maximum_threshold = {
                let first_bid_threshold = bid_thresholds[0];
                let thresholds = bid_thresholds[1..].iter().chain(ask_thresholds.iter());
                crate::state::maximum_threshold(&thresholds, first_bid_threshold)
            };
            if settings_words[CURVE_PARAMETERS / 8] > maximum_threshold {
                return Err(GoonFiError::InvalidQuoteThresholds.into());
            }
            if bid_rate_millionths > 1_000_000 {
                return Err(GoonFiError::BidRateOutOfRange.into());
            }
            if ask_rate_millionths > 1_000_000 {
                return Err(GoonFiError::AskRateOutOfRange.into());
            }
            if smoothing_mode > 1
                || (smoothing_mode == 1 && !(1..=u32::MAX as u64).contains(&smoothing_period))
            {
                return Err(GoonFiError::InvalidSmoothingConfiguration.into());
            }

            Ok(())
        };
        validate_configuration()?;

        let market_key = market.key();
        if bid_mint.key() == ask_mint.key() {
            return Err(GoonFiError::IdenticalMarketMints.into());
        }
        let bid_decimals: Result<u64, ProgramError> =
            if bid_program.key() == &pubkey::TOKEN_PROGRAM_ID {
                get_legacy_bid_decimals(bid_mint).map(u64::from)
            } else if bid_program.key() == &pubkey::TOKEN_2022_PROGRAM_ID {
                decode_token2022_decimals(bid_mint).map(u64::from)
            } else {
                Err(ProgramError::IncorrectProgramId)
            };
        let bid_decimals = bid_decimals? as u8;
        let ask_decimals = get_ask_decimals(ask_mint, ask_program)?;
        if (parameters.bid_decimals, parameters.ask_decimals) != (bid_decimals, ask_decimals) {
            return Err(GoonFiError::MintDecimalsMismatch.into());
        }

        let ask_vault_bump;
        {
            let market_bump = [parameters.market_bump];
            let market_id = parameters.market_id;
            let market_id_bytes = market_id.to_le_bytes();
            let market_seeds = [
                Seed::from(&b"market_v2"[..]),
                Seed::from(bid_mint.key()),
                Seed::from(ask_mint.key()),
                Seed::from(&market_id_bytes),
                Seed::from(&market_bump),
            ];
            create_or_allocate_account(
                market,
                &pubkey::ID,
                MARKET_ACCOUNT_LEN as u64,
                &market_seeds,
                payer,
                None,
            )?;
            {
                let mut data = market.try_borrow_mut_data()?;
                let state = MarketConfigurationState::initialize_account(&mut data);
                let body = bytemuck::bytes_of_mut(state);

                let keys = bytemuck::from_bytes_mut::<[Pubkey; 6]>(&mut body[0x28..0xe8]);
                keys[0] = *payer.key();
                keys[1] = *bid_mint.key();
                keys[2] = *ask_mint.key();
                keys[3] = *bid_vault.key();
                keys[4] = *ask_vault.key();
                keys[5] = *oracle.key();
                body[0x7b4] = bid_decimals;
                body[0x7b5] = ask_decimals;
                body[0xe8..0x7b0].copy_from_slice(bytemuck::bytes_of(settings_words));
                body[0x7b0] = market_bump[0];
                let words: &mut [u64; MARKET_STATE_WORDS] = bytemuck::cast_mut(state);
                words[0] = market_id;
                let body = bytemuck::bytes_of_mut(words);
                ask_vault_bump = parameters.ask_vault_bump;
                body[0x7b1] = ask_vault_bump;
                body[0x7b2] = parameters.bid_vault_bump;
                body[0x7b6] = 1;
                let programs = bytemuck::from_bytes_mut::<[Pubkey; 2]>(&mut body[0x7b8..0x7f8]);
                programs[0] = *bid_program.key();
                programs[1] = *ask_program.key();
            }
        }

        {
            let bid_bump = [parameters.bid_vault_bump];
            let bid_seeds = [
                Seed::from(&crate::state::market_v2::VAULT_SEED[..]),
                Seed::from(market_key),
                Seed::from(bid_mint.key()),
                Seed::from(&bid_bump),
            ];
            let bid_space = token::token_account_space(bid_mint, bid_program.key())? as u64;
            create_or_allocate_account(
                bid_vault,
                bid_program.key(),
                bid_space,
                &bid_seeds,
                payer,
                None,
            )?;
            pinocchio_token_2022::instructions::InitializeAccount3 {
                account: bid_vault,
                mint: bid_mint,
                owner: market_key,
                token_program: bid_program.key(),
            }
            .invoke()?;
        }

        let ask_bump = [ask_vault_bump];
        let ask_seeds = [
            Seed::from(&crate::state::market_v2::VAULT_SEED[..]),
            Seed::from(market_key),
            Seed::from(ask_mint.key()),
            Seed::from(&ask_bump),
        ];
        let ask_space = token::token_account_space(ask_mint, ask_program.key())? as u64;
        create_or_allocate_account(
            ask_vault,
            ask_program.key(),
            ask_space,
            &ask_seeds,
            payer,
            None,
        )?;
        pinocchio_token_2022::instructions::InitializeAccount3 {
            account: ask_vault,
            mint: ask_mint,
            owner: market_key,
            token_program: ask_program.key(),
        }
        .invoke()?;
        Ok(())
    }

    // Stage the pricing tiers and settings image for market initialization.
    let initial_tiers: [PricingTier; 7];
    let settings_words = core::mem::MaybeUninit::<[u64; SETTINGS_BYTES / 8]>::uninit();
    prologue_padding!(5, 4);

    let class_tiers = [
        parameters.class_0,
        parameters.class_1,
        parameters.class_2,
        parameters.class_3,
        parameters.class_4,
        parameters.class_5,
        parameters.class_6,
        parameters.class_7,
        parameters.class_8,
        parameters.class_9,
        parameters.class_10,
        parameters.class_11,
        parameters.class_12,
        parameters.class_13,
        parameters.class_14,
        parameters.class_15,
    ];
    for &class in &class_tiers {
        if class > 6 {
            return Err(GoonFiError::InvalidPricingTier.into());
        }
    }
    initial_tiers = [parameters.initial_tier; 7];
    let accounts = MarketAccounts {
        payer,
        market,
        bid_mint,
        ask_mint,
        bid_vault,
        ask_vault,
        oracle,
        bid_program,
        ask_program,
    };
    finish_create_market(
        accounts,
        parameters,
        initial_tiers,
        settings_words,
        class_tiers,
    )?;
    // Return success after the market and both vaults have been initialized.
    Ok(())
}

/// Validates and applies all mutable market settings as one update.
#[inline(never)]
pub(crate) fn apply_market_configuration(
    market: &mut MarketConfigurationState,
    configuration: &MarketConfigurationInput,
) -> ProgramResult {
    prologue_padding!(8, 3);
    fn validate_geometry(configuration: &MarketConfigurationInput) -> ProgramResult {
        if configuration.near_bid_denominator > configuration.near_ask_denominator
            || configuration.far_bid_denominator > configuration.far_ask_denominator
        {
            return Err(GoonFiError::InvalidInventoryDenominatorOrder.into());
        }
        if configuration.inventory_limit <= configuration.inventory_knee {
            return Err(GoonFiError::InvalidInventoryRange.into());
        }
        Ok(())
    }

    // Leading zero thresholds may precede a strictly increasing nonzero suffix.
    #[inline(always)]
    fn validate_threshold_sequence(thresholds: &[u64; 10]) -> bool {
        struct SequenceState {
            previous: u64,
            started: bool,
        }

        let mut values = thresholds.iter().copied();
        let mut state = SequenceState {
            previous: 0,
            started: false,
        };
        let mut invalid = false;
        while let Some(value) = values.next() {
            if value == 0 {
                invalid = state.started || invalid;
                continue;
            }
            state.started = true;
            let out_of_order = value <= state.previous;
            state.previous = value;
            invalid |= out_of_order;
            if invalid {
                return false;
            }
        }
        !invalid
    }

    if configuration.bid_token_program.as_slice() != pubkey::TOKEN_PROGRAM_ID.as_slice()
        && configuration.bid_token_program.as_slice() != pubkey::TOKEN_2022_PROGRAM_ID.as_slice()
    {
        return Err(GoonFiError::UnsupportedMarketTokenProgram.into());
    }
    if configuration.ask_token_program != pubkey::TOKEN_PROGRAM_ID
        && configuration.ask_token_program != pubkey::TOKEN_2022_PROGRAM_ID
    {
        return Err(GoonFiError::UnsupportedMarketTokenProgram.into());
    }

    let near_bid_denominator = configuration.near_bid_denominator;
    let near_ask_denominator = configuration.near_ask_denominator;
    let far_bid_denominator = configuration.far_bid_denominator;
    let far_ask_denominator = configuration.far_ask_denominator;
    let inventory_limit = configuration.inventory_limit;
    let inventory_knee = configuration.inventory_knee;
    let maximum_quote_age = configuration.maximum_quote_age;
    let grace_slots = configuration.grace_slots;
    validate_geometry(configuration)?;
    if maximum_quote_age > 200 {
        return Err(GoonFiError::InvalidMaximumQuoteAge.into());
    }
    if grace_slots > 50 {
        return Err(GoonFiError::InvalidStalenessConfiguration.into());
    }

    let class_0 = configuration.class_0;
    let class_1 = configuration.class_1;
    let class_2 = configuration.class_2;
    let class_3 = configuration.class_3;
    let class_4 = configuration.class_4;
    let class_5 = configuration.class_5;
    let class_6 = configuration.class_6;
    let class_7 = configuration.class_7;
    let class_8 = configuration.class_8;
    let class_9 = configuration.class_9;
    let class_10 = configuration.class_10;
    let class_11 = configuration.class_11;
    let class_12 = configuration.class_12;
    let class_13 = configuration.class_13;
    let class_14 = configuration.class_14;
    let class_15 = configuration.class_15;
    if [
        class_0, class_1, class_2, class_3, class_4, class_5, class_6, class_7, class_8, class_9,
        class_10, class_11, class_12, class_13, class_14, class_15,
    ]
    .iter()
    .copied()
    .any(|tier| tier > 6)
    {
        return Err(GoonFiError::InvalidPricingTier.into());
    }

    if !validate_threshold_sequence(&configuration.bid_thresholds)
        || !validate_threshold_sequence(&configuration.ask_thresholds)
        || {
            let first_bid_threshold = configuration.bid_thresholds[0];
            let thresholds = configuration.bid_thresholds[1..]
                .iter()
                .chain(configuration.ask_thresholds.iter());
            let maximum_threshold =
                crate::state::maximum_threshold(&thresholds, first_bid_threshold);
            configuration.minimum_quote_threshold > maximum_threshold
        }
    {
        return Err(GoonFiError::InvalidQuoteThresholds.into());
    }
    let minimum_quote_threshold = configuration.minimum_quote_threshold;

    let bid_rate_millionths = configuration.bid_rate_millionths;
    if bid_rate_millionths > 1_000_000 {
        return Err(GoonFiError::BidRateOutOfRange.into());
    }
    let ask_rate_millionths = configuration.ask_rate_millionths;
    if ask_rate_millionths > 1_000_000 {
        return Err(GoonFiError::AskRateOutOfRange.into());
    }
    let smoothing_mode = configuration.smoothing_mode;
    let smoothing_period = configuration.smoothing_period;
    if smoothing_mode > 1
        || (smoothing_mode == 1 && !(1..=u32::MAX as u64).contains(&smoothing_period))
    {
        return Err(GoonFiError::InvalidSmoothingConfiguration.into());
    }

    let bytes = bytemuck::bytes_of_mut(market);
    bytes[0x5b8..0x608].copy_from_slice(bytemuck::bytes_of(&configuration.bid_thresholds));
    bytes[0x608..0x658].copy_from_slice(bytemuck::bytes_of(&configuration.ask_thresholds));
    // Class selectors occupy two separate regions in the market layout.
    bytes[0x692] = class_0;
    bytes[0x693] = class_1;
    bytes[0x694] = class_2;
    bytes[0x695] = class_3;
    bytes[0x696] = class_4;
    bytes[0x697] = class_5;
    bytes[0x698] = class_6;
    bytes[0x699] = class_7;
    bytes[0x69a] = class_8;
    bytes[0x69b] = class_9;
    bytes[0x69c] = class_10;
    bytes[0x69d] = class_11;
    bytes[0x69e] = class_12;
    bytes[0x69f] = class_13;
    bytes[0x6c0] = class_14;
    bytes[0x6c1] = class_15;
    bytes[0x658..0x660].copy_from_slice(&minimum_quote_threshold.to_le_bytes());
    bytes[0x660..0x668].copy_from_slice(&near_bid_denominator.to_le_bytes());
    bytes[0x668..0x670].copy_from_slice(&near_ask_denominator.to_le_bytes());
    bytes[0x670..0x678].copy_from_slice(&inventory_knee.to_le_bytes());
    bytes[0x678..0x680].copy_from_slice(&far_bid_denominator.to_le_bytes());
    bytes[0x680..0x688].copy_from_slice(&far_ask_denominator.to_le_bytes());
    bytes[0x688..0x690].copy_from_slice(&inventory_limit.to_le_bytes());
    bytes[0x690] = maximum_quote_age;
    bytes[0x691] = grace_slots;
    bytes[0x7b8..0x7d8].copy_from_slice(&configuration.bid_token_program);
    bytes[0x7d8..0x7f8].copy_from_slice(&configuration.ask_token_program);
    bytes[0x6a0..0x6a8].copy_from_slice(&bid_rate_millionths.to_le_bytes());
    bytes[0x6b8..0x6c0].copy_from_slice(&ask_rate_millionths.to_le_bytes());
    bytes[0x6d0..0x6d8].copy_from_slice(&smoothing_period.to_le_bytes());
    bytes[0x6d8..0x6e0].copy_from_slice(&configuration.smoothing_interval.to_le_bytes());
    bytes[0x6f8] = smoothing_mode;
    bytes[0x6f9] = configuration.disable_staleness_adjustment;
    bytes[0x6fa] = configuration.quote_flags;
    Ok(())
}
