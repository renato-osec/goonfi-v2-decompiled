use crate::route_tables::RECOGNIZED_ROUTE_SIGNERS_RUNTIME;
use goonfi_common::{helpers::account_count, pricing_class::PricingClass};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, sysvars::instructions::Instructions};

/// Signer selected for route kind 2.
pub const PRIMARY_ROUTE_SIGNER: Pubkey = [
    0xd1, 0xc9, 0xbe, 0x00, 0x9f, 0x1d, 0xc5, 0x15,
    0x99, 0xc7, 0x98, 0x8b, 0x4e, 0xaa, 0x85, 0x69,
    0x8f, 0x47, 0x4d, 0xfd, 0xf4, 0xcd, 0x7c, 0x3c,
    0xc5, 0x4d, 0x72, 0x36, 0x92, 0x3f, 0x6e, 0xf6,
];

/// Signer selected for the remaining route kinds.
pub const ALTERNATE_ROUTE_SIGNER: Pubkey = [
    0x0c, 0xfe, 0x2c, 0x57, 0xed, 0x76, 0x01, 0x33,
    0x0f, 0x8e, 0x98, 0x13, 0x07, 0x3d, 0xfe, 0xae,
    0x27, 0xb2, 0x15, 0x93, 0x21, 0xe3, 0x14, 0x4f,
    0x15, 0x05, 0x06, 0xad, 0xdf, 0x90, 0xae, 0x72,
];

/// Classifies a signed route from its current instruction and transaction signers.
///
/// Classes 11 and 12 identify the two recognized signer groups; None means unclassified.
pub fn classify_signed_route(
    account: &AccountInfo,
    route_kind: u32,
) -> Result<Option<PricingClass>, ProgramError> {
    let (signer, class) = if route_kind == 2 {
        (&PRIMARY_ROUTE_SIGNER, PricingClass::TitanQuote)
    } else {
        (&ALTERNATE_ROUTE_SIGNER, PricingClass::StandardQuote)
    };
    let instructions = Instructions::try_from(account)?;
    let current = instructions.get_instruction_relative(0)?;
    let count = account_count(&current);
    for index in 0..count {
        let meta = current.get_account_meta_at(index)?;
        if !meta.is_signer() {
            continue;
        }
        if &meta.key == signer {
            return Ok(Some(class));
        }
    }
    match route_kind {
        2 => Ok(None),
        _ => {
            let recognized = transaction_has_recognized_signer(account)?;
            if recognized {
                return Ok(Some(PricingClass::StandardQuote));
            }
            Ok(None)
        }
    }
}

/// Finds a recognized route signer in any transaction instruction.
pub fn transaction_has_recognized_signer(
    account: &AccountInfo,
) -> Result<bool, ProgramError> {
    let instructions = Instructions::try_from(account)?;
    let instruction_count = usize::from(instructions.num_instructions());
    let mut index = 0;
    loop {
        if index >= instruction_count {
            break;
        }
        let current_index = index;
        index += 1;
        // SAFETY: the index was checked against the sysvar instruction count.
        let instruction =
            unsafe { instructions.deserialize_instruction_unchecked(current_index) };
        let mut account_index = 0;
        while let Ok(meta) = instruction.get_account_meta_at(account_index) {
            account_index += 1;
            if meta.is_signer() {
                if RECOGNIZED_ROUTE_SIGNERS_RUNTIME.contains(&meta.key) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
