use crate::{
    account_info::AccountInfo,
    internal::{ProgramError, ProgramResult},
};

pub mod restart;
pub mod bloom;
pub mod market_lifecycle;
pub use crate::state::bloom_queries;
pub mod liquidity;
pub mod market_ops;
pub mod quote;
pub mod swap;
pub mod swap_with_route;
pub mod market_admin;
pub mod whitelisted_key_registry;

use bloom::{add_bloom_entry, close_bloom_filter, remove_bloom_entry};
use liquidity::*;
use market_admin::{set_pricing_tier, update_configuration};
use market_lifecycle::{close_market, initialize_market};
use market_ops::*;
use restart::sync_restart_slot;
use quote::*;
use swap::*;
use swap_with_route::*;
use whitelisted_key_registry::*;

/// Routes an instruction tag validated by `process_instruction` to its handler.
#[inline(never)]
pub(crate) fn dispatch_instruction(
    tag: u8,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    match tag {
        0 => initialize_market(accounts, data),
        1 => swap(accounts, data),
        2 => deposit(accounts, data),
        3 => withdraw(accounts, data),
        4 => update_configuration(accounts, data),
        5 => close_market(accounts),
        6 => set_pricing_tier(accounts, data),
        7 => add_bloom_entry(accounts, data),
        8 => remove_bloom_entry(accounts, data),
        9 => close_bloom_filter(accounts),
        10 => disable_market_quotes(accounts, data),
        11 => enable_market_quotes(accounts, data),
        12 => refresh_market(accounts, data),
        13 => {
            let _ = quote_swap(accounts, data);
            Ok(())
        }
        14 => swap_with_route(accounts, data),
        15 => init_whitelist(accounts, data),
        16 => replace_whitelist(accounts, data),
        17 => append_whitelist(accounts, data),
        18 => sync_restart_slot(accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
