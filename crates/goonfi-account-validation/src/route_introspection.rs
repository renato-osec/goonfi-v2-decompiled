use pinocchio::pubkey::Pubkey;
use goonfi_common::helpers::{account_count, RouterInstruction};
use pinocchio::{program_error::ProgramError, sysvars::instructions::{IntrospectedAccountMeta, IntrospectedInstruction}};

struct RouteSignerCursor<'a> {
    raw: *const u8,
    offset: usize,
    end: usize,
    index: usize,
    count: usize,
    _marker: core::marker::PhantomData<&'a IntrospectedAccountMeta>,
}

impl<'a> Iterator for RouteSignerCursor<'a> {
    type Item = (usize, &'a IntrospectedAccountMeta);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.offset;
        if current == self.end {
            return None;
        }
        self.offset = current.wrapping_add(core::mem::size_of::<IntrospectedAccountMeta>());
        self.index = self.index.wrapping_add(1);
        // SAFETY: current is within the count-bounded metadata array, and
        // IntrospectedAccountMeta has byte alignment in the borrowed instruction.
        let account = unsafe {
            &*self.raw.add(current).add(2).cast::<IntrospectedAccountMeta>()
        };
        Some((self.index, account))
    }
}

struct RouteTailFlags {
    ptr: *const u8,
    index: usize,
    count: usize,
    active: bool,
}

impl Iterator for RouteTailFlags {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        if !self.active {
            return None;
        }
        self.index = self.index.wrapping_add(1);
        if self.index >= self.count {
            return None;
        }
        // SAFETY: the cursor starts at the meta after the first signer, and the
        // index check keeps this byte-aligned record within the account count.
        let account = unsafe { &*self.ptr.cast::<IntrospectedAccountMeta>() };
        // SAFETY: one record past the current meta remains inside the instruction.
        self.ptr = unsafe { self.ptr.add(core::mem::size_of::<IntrospectedAccountMeta>()) };
        Some(account.is_signer())
    }
}

impl RouteTailFlags {
    // Retain the first signer only while no later account is a signer.
    fn fold_route<B, F>(&mut self, mut value: B, mut f: F) -> Result<B, ProgramError>
    where
        F: FnMut(B, bool) -> Result<B, ProgramError>,
    {
        while let Some(flag) = self.next() {
            value = f(value, flag)?;
        }
        Ok(value)
    }
}

type RouteSignerCandidate = Option<Result<Option<Pubkey>, ProgramError>>;

impl<'a> RouteSignerCursor<'a> {
    fn new(instruction: &'a IntrospectedInstruction<'_>, count: usize) -> Self {
        Self {
            raw: instruction.raw,
            offset: 0,
            end: count * core::mem::size_of::<IntrospectedAccountMeta>(),
            index: usize::MAX,
            count,
            _marker: core::marker::PhantomData,
        }
    }

    fn finish_signer_scan(
        self,
        found: Option<(usize, &'a IntrospectedAccountMeta)>,
    ) -> Result<RouteSignerCandidate, ProgramError> {
        let (first, found_index, ptr) = match found {
            Some((index, account)) => (
                Some(Ok::<Option<Pubkey>, ProgramError>(Some(account.key))),
                index,
                // SAFETY: offset is at most the end of the metadata array; the
                // two-byte header and following program ID are in the instruction.
                unsafe { self.raw.add(self.offset).add(2) },
            ),
            None => (None, self.index, self.raw),
        };
        let mut tail = RouteTailFlags {
            ptr,
            index: found_index,
            count: self.count,
            active: first.is_some(),
        };
        tail.fold_route(first, |first, is_signer| {
            (!is_signer)
                .then_some(first)
                .ok_or(ProgramError::InvalidArgument)
        })
    }
}

/// Finds the route authority, preferring the sole signer for ambiguous layouts.
pub fn route_authority(
    kind: RouterInstruction,
    instruction: &IntrospectedInstruction<'_>,
) -> Result<Option<Pubkey>, ProgramError> {
    use RouterInstruction::*;
    if matches!(kind, Settler) {
        return Ok(None);
    }
    if matches!(kind, DflowAggregatorV4 | BinanceWalletRouter | BinanceWalletVault) {
        let count = account_count(instruction);
        let mut cursor = RouteSignerCursor::new(instruction, count);
        let found = cursor.find(|(_, account)| account.is_signer());
        // Ambiguous signer sets defer to the layout-specific account below.
        let folded = cursor.finish_signer_scan(found);
        let sole = folded.ok().flatten().and_then(Result::ok).flatten();
        let selected = sole.map(|key| Ok(Some(key)));
        if let Some(result) = selected {
            return result;
        }
    }
    let authority_index = match kind {
        JupiterRoute => 1,
        JupiterSharedRoute => 2,
        JupiterRouteV2 => 0,
        JupiterSharedRouteV2 => 1,
        OkxDexV2 => 0,
        Titan => 0,
        TitanV2 => 1,
        Pro => 0,
        DflowAggregatorV4 => 3,
        BinanceWalletRouter => 1,
        BinanceWalletVault => 0,
        SecondaryRouter => 0,
        Settler => 0,
        Sun => 2,
    };
    let authority = instruction.get_account_meta_at(authority_index)?;
    if !authority.is_signer() {
        return Ok(None);
    }
    Ok(Some(authority.key))
}
