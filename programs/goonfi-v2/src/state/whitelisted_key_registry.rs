use crate::{
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::{self, Pubkey},
};
use pinocchio::account_info::{AccountInfo, Ref};

pub const WHITELIST_REGISTRY_ID: Pubkey = [
    0xf8, 0x75, 0x08, 0xf9, 0x72, 0xf5, 0xbe, 0xa0, 0x21, 0x96, 0x8c, 0x16, 0x5c, 0x8e, 0x12, 0x6e,
    0x40, 0x52, 0xc1, 0xe6, 0x07, 0x6c, 0x0b, 0xde, 0x1b, 0x1a, 0x84, 0x5b, 0x3d, 0x24, 0xb9, 0x92,
];
pub const WHITELIST_ACCOUNT_LEN: usize = 0x2398;
pub const WHITELIST_DISCRIMINATOR: u64 = 0xe57a_e6d9_ab10_5c71;
pub const WHITELIST_DISCRIMINATOR_BYTES: [u8; 8] = WHITELIST_DISCRIMINATOR.to_le_bytes();
pub const MAX_WHITELIST_PROGRAMS: usize = 16;
pub const MAX_WHITELIST_ENTRIES: usize = 256;

/// One serialized program entry in the whitelist registry.
///
/// The address is followed by an eight-byte permission mask.
/// Its bits authorize individual program operations.
#[derive(Clone, Copy, borsh::BorshDeserialize, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct AccessEntry {
    pub key: Pubkey,
    pub permissions: u64,
}

impl AccessEntry {
    fn address(&self) -> &Pubkey {
        &self.key
    }
}

impl WhitelistedKeyRegistry {
    /// Mutably borrows the complete, aligned whitelist account layout.
    /// The caller passes the body after its eight-byte discriminator.
    #[inline]
    pub fn whitelist_account_from_bytes(data: &mut [u8]) -> &mut Self {
        bytemuck::from_bytes_mut(data)
    }
}

/// One 40-byte serialized whitelist update record.
///
/// Replace and append compare `(kind, key)` pairs for uniqueness.
/// The same key may occur once in each of the two supported kinds.
/// Borsh consumes the seven trailing bytes in every entry.
/// Rust field layout does not define this Borsh format.
/// Only the key and kind are written into the registry arrays.
#[derive(borsh::BorshDeserialize)]
pub struct WithdrawalEntry {
    key: Pubkey,
    kind: u8,
    _padding: [u8; 7], // Consumed by Borsh; ignored by registry updates.
}

impl WithdrawalEntry {
    // Borrow the key for the kind-specific duplicate scan.
    fn address(&self) -> &Pubkey {
        &self.key
    }

    // Kind selects the registry namespace used by duplicate checks.
    fn kind(&self) -> u8 {
        self.kind
    }
}

/// Validated whitelist account data, excluding the account discriminator.
/// Program permissions and key-kind entries have separate fixed capacities.
/// Stored counts select the active prefix of each array.
/// The bump slot completes the aligned account body.
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct WhitelistedKeyRegistry {
    programs: [AccessEntry; MAX_WHITELIST_PROGRAMS],
    addresses: [Pubkey; MAX_WHITELIST_ENTRIES],
    kinds: [u8; MAX_WHITELIST_ENTRIES],
    program_count: u32,
    entry_count: u32,
    /// One bump byte and seven reserved bytes fill the account slot.
    bump: PdaBumpSlot,
}

impl WhitelistedKeyRegistry {
    /// Includes slots beyond `program_count`; `active_programs` limits the slice.
    pub fn programs(&self) -> &[AccessEntry; MAX_WHITELIST_PROGRAMS] {
        &self.programs
    }

    /// Clears a newly allocated registry and records its PDA bump.
    #[inline]
    pub fn initialize(&mut self, bump: u8) {
        self.programs = [AccessEntry {
            key: [0; 32],
            permissions: 0,
        }; MAX_WHITELIST_PROGRAMS];
        self.addresses = [[0; 32]; MAX_WHITELIST_ENTRIES];
        self.kinds = [0; MAX_WHITELIST_ENTRIES];
        self.program_count = 0;
        self.entry_count = 0;
        self.bump.bump = bump;
        self.bump.reserved.fill(0);
    }

    /// Addresses pair by index with kinds; entries beyond `entry_count` are inactive.
    pub fn addresses(&self) -> &[Pubkey; MAX_WHITELIST_ENTRIES] {
        &self.addresses
    }

    /// Each kind classifies the address at the same array index.
    pub fn kinds(&self) -> &[u8; MAX_WHITELIST_ENTRIES] {
        &self.kinds
    }

    /// Borrows an initialized whitelist account.
    /// The returned guard keeps the account data borrowed while it is in use.
    #[inline(always)]
    pub fn borrow_account(account: &AccountInfo) -> Result<Ref<'_, Self>, ProgramError> {
        if account.data_len() != WHITELIST_ACCOUNT_LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        if !account.is_owned_by(&pubkey::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let data = account.try_borrow_data()?;
        if data[..8] != WHITELIST_DISCRIMINATOR_BYTES {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(Ref::map(data, |data| {
            bytemuck::from_bytes(&data[8..WHITELIST_ACCOUNT_LEN])
        }))
    }

    /// `active_programs` clamps this stored count before slicing.
    pub fn program_count(&self) -> u32 {
        self.program_count
    }

    /// Append operations clamp this count to capacity before adding entries.
    pub fn entry_count(&self) -> u32 {
        self.entry_count
    }

    /// Replaces the whitelist after validating entry kinds and key-kind uniqueness.
    /// Existing entries remain intact if validation fails.
    pub fn replace_whitelist_entries(&mut self, entries: &[WithdrawalEntry]) -> ProgramResult {
        if entries.len() > MAX_WHITELIST_ENTRIES {
            return Err(GoonFiError::InvalidWhitelistUpdate.into());
        }
        for index in 0..entries.len() {
            let entry = &entries[index];
            if !(WHITELIST_KIND_LIQUIDITY_ACCOUNT..=WHITELIST_KIND_TOKEN_OWNER)
                .contains(&entry.kind()) {
                return Err(ProgramError::InvalidInstructionData);
            }
            for previous_index in 0..index {
                let previous = &entries[previous_index];
                if entry.kind() != previous.kind() {
                    continue;
                }
                if entry.address() == previous.address() {
                    return Err(GoonFiError::InvalidWhitelistUpdate.into());
                }
            }
        }
        self.addresses = [[0; 32]; MAX_WHITELIST_ENTRIES];
        self.kinds = [0; MAX_WHITELIST_ENTRIES];
        for (index, entry) in entries.iter().enumerate() {
            self.addresses[index] = *entry.address();
            self.kinds[index] = entry.kind();
        }
        self.entry_count = entries.len() as u32;
        Ok(())
    }

    /// Appends distinct key-kind entries without changing the existing registry.
    /// Only the two declared key kinds are accepted.
    /// Capacity and duplicate checks finish before any entry is appended.
    pub fn append_whitelist_entries(&mut self, entries: &[WithdrawalEntry]) -> ProgramResult {
        let stored_count = self.entry_count as usize;
        let existing = stored_count.min(MAX_WHITELIST_ENTRIES);
        let total = match existing.checked_add(entries.len()) {
            Some(total) if total <= MAX_WHITELIST_ENTRIES => total,
            _ => return Err(GoonFiError::InvalidWhitelistUpdate.into()),
        };
        for index in 0..entries.len() {
            let entry = &entries[index];
            if !(WHITELIST_KIND_LIQUIDITY_ACCOUNT..=WHITELIST_KIND_TOKEN_OWNER)
                .contains(&entry.kind())
            {
                return Err(ProgramError::InvalidInstructionData);
            }
            for previous_index in 0..existing {
                let address = &self.addresses[previous_index];
                if self.kinds[previous_index] == entry.kind()
                    && address == entry.address()
                {
                    return Err(GoonFiError::InvalidWhitelistUpdate.into());
                }
            }
            for previous_index in 0..index {
                if entry.kind() == entries[previous_index].kind()
                    && entry.address() == entries[previous_index].address()
                {
                    return Err(GoonFiError::InvalidWhitelistUpdate.into());
                }
            }
        }
        for (index, entry) in entries.iter().enumerate() {
            let destination = existing + index;
            let address = *entry.address();
            self.addresses[destination] = address;
            self.kinds[destination] = entry.kind();
        }
        self.entry_count = total as u32;
        Ok(())
    }

    /// Replaces the program list after rejecting duplicate addresses.
    pub fn replace_whitelisted_programs(
        &mut self,
        entries: &[AccessEntry],
    ) -> ProgramResult {
        if entries.len() > MAX_WHITELIST_PROGRAMS {
            return Err(GoonFiError::InvalidWhitelistUpdate.into());
        }
        for index in 0..entries.len() {
            for previous in 0..index {
                if entries[index].address() == entries[previous].address() {
                    return Err(GoonFiError::InvalidWhitelistUpdate.into());
                }
            }
        }
        self.programs.fill(AccessEntry {
            key: [0; 32],
            permissions: 0,
        });
        self.programs[..entries.len()].copy_from_slice(entries);
        self.program_count = entries.len() as u32;
        Ok(())
    }

    /// Returns the program records counted by this registry.
    pub fn active_programs(&self) -> &[AccessEntry] {
        let count = (self.program_count as usize).min(MAX_WHITELIST_PROGRAMS);
        &self.programs[..count]
    }

}

/// Whitelist entry for a liquidity account key.
pub const WHITELIST_KIND_LIQUIDITY_ACCOUNT: u8 = 1;
/// Whitelist entry for a token owner key.
pub const WHITELIST_KIND_TOKEN_OWNER: u8 = 2;

/// The account stores one PDA bump followed by seven reserved bytes.
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct PdaBumpSlot {
    bump: u8,
    reserved: [u8; 0x7],
}

pub use WhitelistedKeyRegistry as Whitelist;
