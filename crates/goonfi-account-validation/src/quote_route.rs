use crate::{route_tables::PRIORITY_ROUTE_SIGNERS, validate_route_trade, RouteSummary};
use goonfi_common::pricing_class::PricingClass;
use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
};

/// Selects the pricing class for a routed quote or swap.
/// After authorization checks, the flag selects the unclassified tier for Jupiter routes.
pub fn classify_quote_route(
    signer: &Pubkey,
    force_unclassified_jupiter: bool,
    route_class: Option<PricingClass>,
    mode: u8,
    route: &RouteSummary,
    instructions: &AccountInfo,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
) -> Result<PricingClass, ProgramError> {
    if &PRIORITY_ROUTE_SIGNERS[0] == signer || &PRIORITY_ROUTE_SIGNERS[1] == signer {
        return Ok(PricingClass::RecognizedSigner);
    }
    if mode == 1 {
        return Ok(PricingClass::Authorized);
    }
    if route.has_multiple_routes {
        return Ok(PricingClass::Authorized);
    }
    if route.has_unlisted_program {
        return Ok(PricingClass::Authorized);
    }
    if force_unclassified_jupiter && route.is_jupiter {
        return Ok(PricingClass::Unclassified);
    }
    if let Some(class) = route_class {
        return Ok(class);
    }
    let valid = validate_route_trade(route, instructions, base_mint, quote_mint)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    if valid {
        Ok(PricingClass::Authorized)
    } else {
        Ok(route.classification)
    }
}
