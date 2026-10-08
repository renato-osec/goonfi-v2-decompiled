pub const ID: [u8; 32] = pinocchio_pubkey::pubkey!("dijkbkCAKfFTCxQg3u1pg82gVU1jJGHBBRcteD11mBu");

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct Oracle {
    pub bid_price: u64,
    pub ask_price: u64,
    pub slot: u32,
    pub widener: u32,
    pub timestamp_ms: u64,
}

impl Oracle {
    pub const SIZE: usize = core::mem::size_of::<Oracle>();

    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        let bid_price = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let ask_price = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let slot = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let widener = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let timestamp_ms = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        Oracle {
            bid_price,
            ask_price,
            slot,
            widener,
            timestamp_ms,
        }
    }
}
