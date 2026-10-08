pub use pinocchio::pubkey::Pubkey;

/// Compares two account keys.
#[inline]
pub fn equals(left: &Pubkey, right: &Pubkey) -> bool {
    left.as_slice().cmp(right.as_slice()).is_eq()
}
