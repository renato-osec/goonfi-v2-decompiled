use crate::{
    route_classifier::classify_router,
    route_introspection::route_authority,
    route_pubkey as pubkey,
    route_tables::{
        ALLOWED_PROGRAM_IDS, ROUTER_PROGRAM_IDS, JUPITER_V6_ID, SECONDARY_ROUTER_ID,
        OKX_DEX_V2_ID, TITAN_ID, OKX_ROUTER_ID, PREXGY_ID, DFLOW_AGGREGATOR_V4_ID,
        BINANCE_WALLET_ROUTER_ID, BINANCE_WALLET_VAULT_ID, LIQUID_MESH_ROUTER_ID,
        ZERO_X_SETTLER_ID, SUN_ID, BITGET_WALLET_AGGREGATOR_ID,
    },
};
use goonfi_common::{helpers::RouterInstruction, pricing_class::PricingClass as RouteClass};
use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey,
    sysvars::instructions::Instructions,
};

const TITAN_V2_DISCRIMINATOR: u8 = 0x2a;

const UNLISTED_PROGRAM_FLAG: u64 = 1;

#[repr(C)]
pub struct RouteMetadata {
    pub kind: u8,
    pub router: RouterInstruction,
    pub instruction_index: u16,
}

impl RouteMetadata {
    const NONE: Self = Self {
        kind: 0,
        router: RouterInstruction::JupiterRoute,
        instruction_index: 0,
    };
    const UNKNOWN: Self = Self {
        kind: 1,
        router: RouterInstruction::JupiterRoute,
        instruction_index: 0,
    };

    fn parsed(router: RouterInstruction, instruction_index: u16) -> Self {
        Self {
            kind: 2,
            router,
            instruction_index,
        }
    }
}

/// The route selected by the current instruction and transaction-wide checks.
#[repr(C)]
pub struct RouteSummary {
    pub classification: RouteClass,
    pub metadata: RouteMetadata,
    pub authority: Option<Pubkey>,
    pub is_jupiter: bool,
    pub has_multiple_routes: bool,
    pub has_unlisted_program: bool,
}

impl RouteSummary {
    fn no_route(
        metadata: RouteMetadata,
        has_multiple_routes: bool,
        has_unlisted_program: bool,
    ) -> Self {
        Self {
            classification: RouteClass::NoRoute,
            metadata,
            authority: None,
            is_jupiter: false,
            has_multiple_routes,
            has_unlisted_program,
        }
    }
}

/// Inspects the instructions sysvar to identify the active router instruction.
pub fn decode_quote_route(
    instructions_account: &AccountInfo,
) -> Result<RouteSummary, ProgramError> {
    let instructions = Instructions::try_from(instructions_account)?;
    let current_index = instructions.load_current_index();
    let current = instructions.load_instruction_at(current_index as usize)?;
    let mut route_count = 0u8;
    let program_id = current.get_program_id();
    let mut scan_flags = 0u64;
    // Visit the serialized instructions in transaction order.
    // SAFETY: the range yields only indices below the instruction count.
    let mut records = (0..instructions.num_instructions() as u32)
        .map(|index| unsafe { instructions.deserialize_instruction_unchecked(index as usize) });
    while let Some(instruction) = records.next() {
        let program_id = instruction.get_program_id();
        note_unlisted_program(&mut scan_flags, program_id);
        if !ROUTER_PROGRAM_IDS.contains(program_id) {
            continue;
        }
        if let Some(true) = scan_route_status(
            instruction.get_program_id(),
            instruction.get_instruction_data(),
        ) {
            route_count = route_count.saturating_add(1);
        }
    }

    let has_multiple_routes = route_count > 1;
    if !ROUTER_PROGRAM_IDS.contains(program_id) {
        return Ok(RouteSummary::no_route(
            RouteMetadata::NONE,
            has_multiple_routes,
            (scan_flags & UNLISTED_PROGRAM_FLAG) != 0,
        ));
    }

    let current_data = current.get_instruction_data();
    if current_data.len() < 8 {
        return Ok(RouteSummary::no_route(
            RouteMetadata::UNKNOWN,
            has_multiple_routes,
            (scan_flags & UNLISTED_PROGRAM_FLAG) != 0,
        ));
    }

    let Some(tag) = router_instruction(program_id, current_data) else {
        return Ok(RouteSummary::no_route(
            RouteMetadata::UNKNOWN,
            has_multiple_routes,
            (scan_flags & UNLISTED_PROGRAM_FLAG) != 0,
        ));
    };

    let (is_jupiter, router) = match tag {
        ParserTag::JupiterRoute => (true, RouterInstruction::JupiterRoute),
        ParserTag::JupiterSharedRoute => (true, RouterInstruction::JupiterSharedRoute),
        ParserTag::JupiterSharedRouteWithTokenLedger => {
            (true, RouterInstruction::JupiterSharedRoute)
        }
        ParserTag::JupiterRouteV2 => (true, RouterInstruction::JupiterRouteV2),
        ParserTag::JupiterSharedRouteV2 => (true, RouterInstruction::JupiterSharedRouteV2),
        other => (false, other.into_router()),
    };
    let authority = route_authority(router, &current)?;
    let known_classification = match router {
        RouterInstruction::JupiterRoute => Some(RouteClass::Jupiter),
        RouterInstruction::OkxDexV2 | RouterInstruction::Pro => Some(RouteClass::Pro),
        RouterInstruction::Titan | RouterInstruction::TitanV2 => Some(RouteClass::Titan),
        RouterInstruction::DflowAggregatorV4 => Some(RouteClass::PeerMarketMaker),
        RouterInstruction::BinanceWalletRouter | RouterInstruction::BinanceWalletVault => None,
        RouterInstruction::SecondaryRouter => Some(RouteClass::SecondaryRouter),
        RouterInstruction::Settler => Some(RouteClass::Settler),
        RouterInstruction::Sun => Some(RouteClass::Sun),
        _ => Some(RouteClass::JupiterShared),
    };
    let classification = match known_classification {
        Some(classification) => classification,
        None => classify_router(current.clone())?.unwrap_or(RouteClass::RouterSigner),
    };

    Ok(RouteSummary {
        classification,
        metadata: RouteMetadata::parsed(router, current_index),
        authority,
        is_jupiter,
        has_multiple_routes,
        has_unlisted_program: (scan_flags & UNLISTED_PROGRAM_FLAG) != 0,
    })
}

fn scan_route_status(program_id: &Pubkey, data: &[u8]) -> Option<bool> {
    if data.len() < 8 {
        return None;
    }
    Some(router_instruction(program_id, data).is_some())
}

fn note_unlisted_program(scan_flags: &mut u64, program_id: &Pubkey) {
    if !ALLOWED_PROGRAM_IDS.contains(program_id) {
        *scan_flags |= UNLISTED_PROGRAM_FLAG;
    }
}

fn is_sun_discriminator(discriminator: u64) -> bool {
    matches!(
        discriminator,
        0x906a55e62db4602e
            | 0xa227425131af65ed
            | 0xa8ad39805e777cc0
            | 0xa9004438847ab6ad
            | 0xab9815422e9b634d
            | 0xac6cedddc551b683
            | 0xb05d692fc789464c
            | 0xfccf3518a92c1aad
            | 0x3738f757d4ac3594
            | 0x42eb0cf99fd858c7
            | 0x48226272a1c52706
            | 0x79eaebf3028b83ff
    )
}

fn is_dflow_discriminator(discriminator: u64) -> bool {
    matches!(
        discriminator,
        0x885b5beb4c3f4b41
            | 0xa569c4ba92b864de
            | 0xc3c420f16c7f4dcd
            | 0xc88775e1919ec6f8
            | 0xe756017af6d57b5f
            | 0x65879cc54d18aca8
    )
}

#[repr(u8)]
enum ParserTag {
    JupiterRoute,
    JupiterSharedRoute,
    JupiterRouteV2,
    JupiterSharedRouteV2,
    OkxDexV2,
    Titan,
    TitanV2,
    Pro,
    DflowAggregatorV4,
    BinanceWalletRouter,
    BinanceWalletVault,
    SecondaryRouter,
    Settler,
    Sun,
    JupiterSharedRouteWithTokenLedger,
}

impl ParserTag {
    fn into_router(self) -> RouterInstruction {
        match self {
            Self::JupiterRoute => RouterInstruction::JupiterRoute,
            Self::JupiterSharedRoute => RouterInstruction::JupiterSharedRoute,
            Self::JupiterSharedRouteWithTokenLedger => RouterInstruction::JupiterSharedRoute,
            Self::JupiterRouteV2 => RouterInstruction::JupiterRouteV2,
            Self::JupiterSharedRouteV2 => RouterInstruction::JupiterSharedRouteV2,
            Self::OkxDexV2 => RouterInstruction::OkxDexV2,
            Self::Titan => RouterInstruction::Titan,
            Self::TitanV2 => RouterInstruction::TitanV2,
            Self::Pro => RouterInstruction::Pro,
            Self::DflowAggregatorV4 => RouterInstruction::DflowAggregatorV4,
            Self::BinanceWalletRouter => RouterInstruction::BinanceWalletRouter,
            Self::BinanceWalletVault => RouterInstruction::BinanceWalletVault,
            Self::SecondaryRouter => RouterInstruction::SecondaryRouter,
            Self::Settler => RouterInstruction::Settler,
            Self::Sun => RouterInstruction::Sun,
        }
    }
}

fn jupiter_route(discriminator: u64) -> Option<ParserTag> {
    use ParserTag::*;

    match discriminator {
        0x819cd641339b20c1 => Some(JupiterSharedRoute),
        0xaa6a9f77508f79e6 => Some(JupiterSharedRouteWithTokenLedger),
        0x2aade37a97cb17e5 | 0x680e5da774475696 => Some(JupiterRoute),
        0x14afc431ccfa64bb => Some(JupiterRouteV2),
        0xe9d8fe7c935398d1 => Some(JupiterSharedRouteV2),
        _ => None,
    }
}

#[inline(always)]
fn router_instruction(program_id: &Pubkey, data: &[u8]) -> Option<ParserTag> {
    use ParserTag::*;

    if data.len() < 8 {
        return None;
    }
    if pubkey::equals(program_id, &JUPITER_V6_ID) {
        let discriminator = u64::from_le_bytes(data[..8].try_into().unwrap());
        if let Some(router) = jupiter_route(discriminator) {
            return Some(router);
        }
    }
    if pubkey::equals(program_id, &OKX_DEX_V2_ID) {
        let discriminator = u64::from_le_bytes(data[..8].try_into().unwrap());
        if matches!(discriminator, 0x9de0e18ef62cbf0e | 0xaff11fb02126e0f0) {
            return Some(OkxDexV2);
        }
    }
    // Titan's first data byte distinguishes its two route variants.
    if let Some(router) =
        (program_id == &TITAN_ID).then(|| match data[0].cmp(&TITAN_V2_DISCRIMINATOR) {
            core::cmp::Ordering::Equal => TitanV2,
            core::cmp::Ordering::Less | core::cmp::Ordering::Greater => Titan,
        })
    {
        return Some(router);
    }
    if program_id == &OKX_ROUTER_ID || program_id == &PREXGY_ID {
        let discriminator = u64::from_le_bytes(data[..8].try_into().unwrap());
        if matches!(
            discriminator,
            0x827f86a1ca0a5def
                | 0xc88775e1919ec6f8
                | 0xd8364b08f2d70148
                | 0xee2c384894822c13
                | 0x19f106ccead8aadf
                | 0x351f5084b15529aa
                | 0x3cec9b1033d4c9bb
                | 0x5a9fb01adb935c24
                | 0x682f5a17bd6bd67f
                | 0x6ca19a95b0a99cbe
        ) {
            return Some(Pro);
        }
    }
    // The remaining checks compare the DFlow and Sun program IDs.
    let (sun_program_id, dflow_program_id) = (&SUN_ID, &DFLOW_AGGREGATOR_V4_ID);
    if pubkey::equals(program_id, dflow_program_id) {
        let discriminator = u64::from_le_bytes(data[..8].try_into().unwrap());
        if is_dflow_discriminator(discriminator) {
            return Some(DflowAggregatorV4);
        }
    }
    if program_id == &BINANCE_WALLET_ROUTER_ID {
        return Some(BinanceWalletRouter);
    }
    if program_id == &BINANCE_WALLET_VAULT_ID {
        return Some(BinanceWalletVault);
    }
    if program_id == &SECONDARY_ROUTER_ID || program_id == &LIQUID_MESH_ROUTER_ID {
        return Some(SecondaryRouter);
    }
    if pubkey::equals(program_id, &ZERO_X_SETTLER_ID) {
        if data[0] == 0 {
            return Some(Settler);
        }
    }
    if program_id == sun_program_id || program_id == &BITGET_WALLET_AGGREGATOR_ID {
        let discriminator = u64::from_le_bytes(data[..8].try_into().unwrap());
        if is_sun_discriminator(discriminator) {
            return Some(Sun);
        }
    }
    None
}
