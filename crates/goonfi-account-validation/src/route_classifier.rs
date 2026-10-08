use crate::route_tables::{JUPITER_V6_ID, SECONDARY_ROUTER_ID};
use goonfi_common::{
    helpers::account_count,
    pricing_class::PricingClass as RouteClass,
};
use pinocchio::{
    program_error::ProgramError,
    sysvars::instructions::IntrospectedInstruction,
};

/// Classifies a routed swap by the first recognized program in its accounts.
pub fn classify_router(
    instruction: IntrospectedInstruction<'_>,
) -> Result<Option<RouteClass>, ProgramError> {
    let count = account_count(&instruction);
    for index in 0..count {
        let account = instruction.get_account_meta_at(index)?;
        let key = &account.key;
        if key.eq(&JUPITER_V6_ID) {
            return Ok(Some(RouteClass::JupiterShared));
        }
        if key.eq(&SECONDARY_ROUTER_ID) {
            return Ok(Some(RouteClass::SecondaryRouter));
        }
    }
    Ok(None)
}
