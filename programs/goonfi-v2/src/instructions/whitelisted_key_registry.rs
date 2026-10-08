//! Instructions for the whitelist registry.

use crate::{
    account_info::AccountInfo,
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
    state::whitelisted_key_registry::{
        Whitelist, WithdrawalEntry, AccessEntry, MAX_WHITELIST_ENTRIES,
        MAX_WHITELIST_PROGRAMS, WHITELIST_ACCOUNT_LEN,
        WHITELIST_DISCRIMINATOR, WHITELIST_REGISTRY_ID,
    },
    utils::cpi::create_or_allocate_account,
};
use borsh::BorshDeserialize;
use crate::utils::helpers::prologue_padding;
use pinocchio::instruction::Seed;

/// The signing authority permitted to append whitelist entries.
const APPEND_AUTHORITY: Pubkey = [
    0x97, 0x61, 0x5c, 0x81, 0xf3, 0x3d, 0xad, 0x44, 0xb6, 0xf8, 0x44, 0x55, 0xef, 0x93, 0xc7, 0xfc,
    0x65, 0x38, 0x04, 0x4e, 0x36, 0xb9, 0xb1, 0xd0, 0x24, 0x69, 0xfa, 0x67, 0x00, 0xeb, 0x38, 0xb6,
];

/// Rejects a forged registry before borrowing its account data.
fn validate_registry_account(registry: &AccountInfo) -> ProgramResult {
    if registry.key() != &WHITELIST_REGISTRY_ID {
        return Err(ProgramError::InvalidAccountData);
    }
    if registry.data_len() != WHITELIST_ACCOUNT_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    if !registry.is_owned_by(&pubkey::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    Ok(())
}

/// Allocates the bump-seeded registry PDA and writes its discriminator and arrays.
pub fn init_whitelist(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(11, 3);
    let [authority, registry, system_program, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if authority.key() != &APPEND_AUTHORITY || !authority.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    crate::account_info::validate_system_program(system_program)?;

    if registry.key() != &WHITELIST_REGISTRY_ID {
        return Err(ProgramError::InvalidAccountData);
    }
    if registry.is_owned_by(&pubkey::ID) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let bump_seed = [<u8 as borsh::BorshDeserialize>::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?];
    let bump = bump_seed[0];
    {
        let seeds = [
            Seed::from(b"whitelisted_key_registry"),
            Seed::from(&bump_seed),
        ];
        if let Some(error) = create_or_allocate_account(
            registry,
            &pubkey::ID,
            WHITELIST_ACCOUNT_LEN as u64,
            &seeds,
            authority,
            None,
        ).err() {
            return Err(error);
        }
    }
    // Allocation completes before the registry data is borrowed.
    let mut data = registry.try_borrow_mut_data()?;
    data[..8].copy_from_slice(&WHITELIST_DISCRIMINATOR.to_le_bytes());
    let whitelist: &mut Whitelist =
        bytemuck::from_bytes_mut(&mut data[8..WHITELIST_ACCOUNT_LEN]);
    whitelist.initialize(bump);
    Ok(())
}

/// Replaces both registry lists under the append authority.
pub fn replace_whitelist(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(6, 3);
    let [authority, registry, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if authority.key() != &APPEND_AUTHORITY || !authority.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }

    let ReplaceWhitelistData { programs, entries } =
        ReplaceWhitelistData::try_from_slice(instruction_data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
    if programs.len() > MAX_WHITELIST_PROGRAMS || entries.len() > MAX_WHITELIST_ENTRIES {
        return Err(GoonFiError::InvalidWhitelistUpdate.into());
    }

    // Apply both replacements while holding one mutable registry borrow.
    let result = {
        let mut data = borrow_valid_registry(registry)?;
        match checked_payload(&mut data) {
            Ok(payload) => {
                let whitelist = Whitelist::whitelist_account_from_bytes(payload);
                whitelist.replace_whitelisted_programs(&programs)?;
                whitelist.replace_whitelist_entries(&entries)?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    };
    result?;
    Ok(())
}

/// Appends validated key entries to the registry.
pub fn append_whitelist(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(8, 3);
    let [authority, registry, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if authority.key() != &APPEND_AUTHORITY || !authority.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }

    let entries = std::vec::Vec::<WithdrawalEntry>::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    // Check capacity before borrowing the registry for the append.
    let append_result = if entries.len() > MAX_WHITELIST_ENTRIES {
        Err(GoonFiError::InvalidWhitelistUpdate.into())
    } else {
        match borrow_valid_registry(registry) {
            Ok(mut data) => {
                let payload = checked_payload(&mut data)?;
                let whitelist = Whitelist::whitelist_account_from_bytes(payload);
                whitelist.append_whitelist_entries(&entries)
            },
            Err(error) => Err(error),
        }
    };
    append_result?;
    Ok(())
}

#[derive(borsh::BorshDeserialize)]
struct ReplaceWhitelistData {
    programs: std::vec::Vec<AccessEntry>,
    entries: std::vec::Vec<WithdrawalEntry>,
}

fn borrow_valid_registry(
    registry: &AccountInfo,
) -> Result<pinocchio::account_info::RefMut<'_, [u8]>, ProgramError> {
    validate_registry_account(registry)?;
    registry.try_borrow_mut_data()
}

fn checked_payload(data: &mut [u8]) -> Result<&mut [u8], ProgramError> {
    if u64::from_le_bytes(data[..8].try_into().unwrap()) != WHITELIST_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(&mut data[8..])
}
