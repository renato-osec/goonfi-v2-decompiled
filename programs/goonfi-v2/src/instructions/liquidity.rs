use crate::{
    account_info::AccountInfo,
    internal::{GoonFiError, ProgramError, ProgramResult},
    pubkey::Pubkey,
    state::{
        token::{read_token_account_identity, TokenAccountIdentity},
        whitelisted_key_registry::{
            Whitelist, MAX_WHITELIST_ENTRIES, WHITELIST_KIND_LIQUIDITY_ACCOUNT,
            WHITELIST_KIND_TOKEN_OWNER,
        },
    },
};

/// Token authority for liquidity owned directly by the market operator.
const LIQUIDITY_AUTHORITY: Pubkey = [
    0x0d, 0x88, 0x0e, 0xee, 0x7b, 0xa8, 0x04, 0x97, 0x27, 0x12, 0xaf, 0x88, 0x3a, 0x68, 0x7c, 0x6e,
    0xbb, 0x50, 0xd3, 0xe8, 0x54, 0x28, 0x2b, 0x87, 0xac, 0xca, 0x77, 0xfa, 0x01, 0xde, 0xd9, 0xba,
];
#[inline(always)]
fn whitelist_contains_key(
    addresses: &[Pubkey; MAX_WHITELIST_ENTRIES],
    kinds: &[u8; MAX_WHITELIST_ENTRIES],
    count: usize,
    kind: u8,
    key: &Pubkey,
) -> bool {
    for index in 0..count {
        if kinds[index] == kind && &addresses[index] == key {
            return true;
        }
    }
    false
}

mod deposit;
pub use deposit::deposit;

/// Checks that a liquidity token account has the expected mint and an allowed authority.
pub fn validate_liquidity_token_account(
    account: &AccountInfo,
    expected_mint: &Pubkey,
    whitelist: Option<&Whitelist>,
    token_program: &Pubkey,
) -> ProgramResult {
    let TokenAccountIdentity { owner, mint } = read_token_account_identity(account, token_program)?;
    if &mint != expected_mint {
        return Err(ProgramError::InvalidAccountData);
    }
    if owner == LIQUIDITY_AUTHORITY {
        return Ok(());
    }
    let account_key = account.key();
    let whitelist = whitelist.ok_or(ProgramError::from(
        GoonFiError::UnauthorizedLiquidityAccount,
    ))?;
    let addresses = whitelist.addresses();
    let kinds = whitelist.kinds();
    let count = whitelist.entry_count().min(MAX_WHITELIST_ENTRIES as u32) as usize;

    if whitelist_contains_key(
        addresses,
        kinds,
        count,
        WHITELIST_KIND_LIQUIDITY_ACCOUNT,
        account_key,
    ) || whitelist_contains_key(addresses, kinds, count, WHITELIST_KIND_TOKEN_OWNER, &owner)
    {
        return Ok(());
    }
    Err(GoonFiError::UnauthorizedLiquidityAccount.into())
}

mod withdraw;

pub use withdraw::withdraw;
