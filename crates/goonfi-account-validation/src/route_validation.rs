use crate::route_tables::{PEER_MARKET_MAKER_IDS, ROUTE_STABLECOIN_MINTS};
use goonfi_common::helpers::{account_count, RouterInstruction};
use pinocchio::{
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::instructions::IntrospectedInstruction,
};

/// Detects a peer market maker anywhere in the instruction's account list.
pub(crate) fn contains_peer_market_maker(
    instruction: &IntrospectedInstruction<'_>,
) -> Result<bool, ProgramError> {
    let count = account_count(instruction);
    if count == 0 {
        return Ok(false);
    }
    for index in 0..count {
        let account = instruction.get_account_meta_at(index)?;
        if PEER_MARKET_MAKER_IDS.contains(&account.key) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Checks whether the route uses the same mint on both sides.
pub fn route_uses_same_mint(
    kind: RouterInstruction,
    instruction: &IntrospectedInstruction<'_>,
) -> Result<bool, ProgramError> {
    use RouterInstruction::*;
    let indices = match kind {
        JupiterRoute => Some((3, 2)),
        JupiterSharedRoute => Some((8, 7)),
        JupiterRouteV2 => Some((4, 3)),
        JupiterSharedRouteV2 => Some((7, 6)),
        OkxDexV2 => Some((4, 3)),
        Titan => Some((4, 2)),
        TitanV2 => Some((4, 3)),
        Pro => Some((4, 3)),
        DflowAggregatorV4 => None,
        BinanceWalletRouter => None,
        BinanceWalletVault => None,
        SecondaryRouter => Some((3, 2)),
        Settler => None,
        Sun => Some((6, 5)),
    };
    let Some((destination_index, source_index)) = indices else {
        return goonfi_common::helpers::route_result_ok(false);
    };
    let source = instruction.get_account_meta_at(source_index)?;
    let destination = &instruction.get_account_meta_at(destination_index)?.key;
    if &source.key != destination {
        return goonfi_common::helpers::route_result_ok(false);
    }
    goonfi_common::helpers::route_result_ok(true)
}

/// Detects a stablecoin route whose mint pair differs from the requested pair.
pub fn route_changes_stablecoin_pair(
    kind: RouterInstruction,
    instruction: &IntrospectedInstruction<'_>,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
) -> Result<bool, ProgramError> {
    use RouterInstruction::*;
    let (destination_index, source_index) = match kind {
        JupiterSharedRoute => (8, 7),
        JupiterRouteV2 => (4, 3),
        JupiterSharedRouteV2 => (7, 6),
        Sun => (6, 5),
        _ => return Ok(false),
    };
    let source_account = instruction.get_account_meta_at(source_index)?;
    let destination_account = instruction.get_account_meta_at(destination_index)?;
    let source = &source_account.key;
    let destination = &destination_account.key;
    if source == destination
        || !is_route_stablecoin(source)
        || !is_route_stablecoin(destination)
    {
        return Ok(false);
    }
    if source == base_mint {
        if destination == quote_mint {
            return Ok(false);
        }
    }
    if source != quote_mint {
        return Ok(true);
    }
    Ok(destination != base_mint)
}

fn is_route_stablecoin(mint: &Pubkey) -> bool {
    ROUTE_STABLECOIN_MINTS.contains(mint)
}
