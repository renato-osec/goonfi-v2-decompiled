use crate::{
    route_validation::{contains_peer_market_maker, route_changes_stablecoin_pair, route_uses_same_mint},
    RouteSummary,
};
use goonfi_common::helpers::RouterInstruction;
use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::instructions::{Instructions, IntrospectedInstruction},
};

/// Checks whether a classified route trades against a peer market maker.
pub fn validate_route_trade(
    route: &RouteSummary,
    instructions: &AccountInfo,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
) -> Result<bool, ProgramError> {
    if route.metadata.kind < 2 {
        return Ok(true);
    }
    let instructions = Instructions::try_from(instructions)?;
    let instruction = instructions.load_instruction_at(route.metadata.instruction_index as usize)?;
    if route_changes_stablecoin_pair(
        route.metadata.router,
        &instruction,
        base_mint,
        quote_mint,
    )? {
        return Ok(true);
    }
    check_route_trade(route.metadata.router, &instruction)
}

fn check_route_trade(
    router: RouterInstruction,
    instruction: &IntrospectedInstruction<'_>,
) -> Result<bool, ProgramError> {
    use RouterInstruction::{BinanceWalletRouter, BinanceWalletVault, DflowAggregatorV4, Settler};
    match router {
        DflowAggregatorV4 | BinanceWalletRouter | BinanceWalletVault | Settler => {
            contains_peer_market_maker(instruction)
        }
        other => {
            if !route_uses_same_mint(other, instruction)? {
                return Ok(false);
            }
            contains_peer_market_maker(instruction)
        }
    }
}
