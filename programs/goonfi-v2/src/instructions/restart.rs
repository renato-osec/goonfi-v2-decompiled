use super::market_ops::{
    MARKET_ACCOUNT_LEN, MARKET_DISCRIMINATOR, RESTART_SLOT_PERMISSION, UPDATE_AUTHORITY,
};
use crate::{
    account_info::AccountInfo,
    internal::{ProgramError, ProgramResult},
    pubkey,
    state::{
        market_v2::MarketConfigurationState,
        whitelisted_key_registry::{
            Whitelist, WHITELIST_REGISTRY_ID,
        },
    },
    utils::helpers::prologue_padding,
};
use std::mem::MaybeUninit;

/// Records the last cluster restart slot for the market.
pub fn sync_restart_slot(accounts: &[AccountInfo], _instruction_data: &[u8]) -> ProgramResult {
    prologue_padding!(10, 1);
    if accounts.len() < 2 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    if let Err(error) = authorize_sync_restart_slot(accounts) {
        return Err(error);
    }

    let market = &accounts[1];
    if market.data_len() != MARKET_ACCOUNT_LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    if !market.is_owned_by(&pubkey::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    let mut data = market.try_borrow_mut_data()?;
    if u64::from_le_bytes(data[..8].try_into().unwrap()) != MARKET_DISCRIMINATOR {
        drop(data);
        return Err(ProgramError::InvalidAccountData);
    }
    let state = bytemuck::from_bytes_mut::<MarketConfigurationState>(&mut data[8..MARKET_ACCOUNT_LEN]);

    let mut restart_slot = MaybeUninit::<u64>::uninit();
    // SAFETY: MaybeUninit<u64> supplies eight writable, aligned bytes for the syscall.
    let status = unsafe {
        pinocchio::syscalls::sol_get_last_restart_slot(restart_slot.as_mut_ptr().cast::<u8>())
    };
    if status != 0 {
        return Err(ProgramError::UnsupportedSysvar);
    }
    // SAFETY: status zero means the syscall initialized the entire u64.
    let restart_slot = unsafe { restart_slot.assume_init() };
    *state.last_restart_slot_mut() = restart_slot;
    Ok(())
}

fn authorize_sync_restart_slot(accounts: &[AccountInfo]) -> ProgramResult {
    let signer = &accounts[0];
    if !signer.is_signer() {
        return Err(ProgramError::IncorrectAuthority);
    }
    if signer.key() == &UPDATE_AUTHORITY {
        return Ok(());
    }
    let registry = accounts
        .iter()
        .find(|account| account.key() == &WHITELIST_REGISTRY_ID)
        .ok_or(ProgramError::IncorrectAuthority)?;
    let denied = Whitelist::borrow_account(registry).map_or(true, |data| {
        let entries = data.programs();
        let count = (data.program_count() as usize).min(entries.len());
        !entries[..count].iter().any(|entry| {
            &entry.key == signer.key()
                && entry.permissions & RESTART_SLOT_PERMISSION != 0
        })
    });
    if denied {
        Err(ProgramError::IncorrectAuthority)
    } else {
        Ok(())
    }
}
