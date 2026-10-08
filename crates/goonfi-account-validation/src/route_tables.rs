use pinocchio::pubkey::Pubkey;
use pinocchio_system::ID as SYSTEM_PROGRAM_ID;
use pinocchio_token::ID as TOKEN_PROGRAM_ID;
use pinocchio_token_2022::ID as TOKEN_2022_PROGRAM_ID;

pub const JUPITER_V6_ID: Pubkey = [
    0x04, 0x79, 0xd5, 0x5b, 0xf2, 0x31, 0xc0, 0x6e, 0xee, 0x74, 0xc5, 0x6e, 0xce, 0x68, 0x15,
    0x07, 0xfd, 0xb1, 0xb2, 0xde, 0xa3, 0xf4, 0x8e, 0x51, 0x02, 0xb1, 0xcd, 0xa2, 0x56, 0xbc,
    0x13, 0x8f,
];

/// Secondary recognized router: HuTkmnrv4zPnArMqpbMbFhfwzTR7xfWQZHH1aQKzDKFZ.
pub const SECONDARY_ROUTER_ID: Pubkey = [
    0xfb, 0x2b, 0x71, 0xcc, 0x3a, 0x4d, 0x02, 0x93, 0x20, 0x96, 0xd5, 0x89, 0x77, 0xb2, 0xf8,
    0xb3, 0x9c, 0x6e, 0x49, 0x3f, 0xbd, 0x46, 0x7c, 0xd9, 0xaa, 0x62, 0x1b, 0x3e, 0x5a, 0xd1,
    0xd1, 0xc4,
];

pub const OKX_DEX_V2_ID: Pubkey = [
    85, 145, 86, 241, 162, 92, 109, 19, 79, 42, 247, 230, 10, 154, 13, 52, 126, 199, 145, 86, 99,
    100, 98, 213, 209, 173, 38, 241, 53, 52, 143, 105,
];
pub const TITAN_ID: Pubkey = [
    6, 169, 156, 110, 18, 231, 14, 187, 53, 24, 92, 20, 76, 75, 144, 110, 24, 255, 176, 10, 71,
    116, 47, 42, 94, 4, 17, 11, 105, 136, 201, 212,
];
pub const OKX_ROUTER_ID: Pubkey = [
    12, 66, 155, 215, 193, 143, 80, 248, 21, 109, 154, 252, 28, 221, 231, 45, 246, 104, 217, 171,
    59, 236, 175, 107, 87, 13, 87, 102, 100, 90, 217, 200,
];
// preXgyMmsTzkYSyp9ms1EgSQCbp87B84bT8kyB21bbB
pub const PREXGY_ID: Pubkey = [
    12, 66, 111, 35, 24, 202, 72, 11, 244, 99, 196, 154, 178, 61, 171, 82, 194, 227, 186, 16, 52,
    212, 252, 104, 179, 92, 234, 237, 109, 47, 72, 86,
];
pub const DFLOW_AGGREGATOR_V4_ID: Pubkey = [
    181, 227, 74, 20, 226, 188, 115, 72, 105, 14, 225, 245, 175, 93, 238, 214, 85, 56, 64, 163,
    109, 170, 184, 96, 176, 80, 96, 115, 189, 192, 60, 16,
];
// B3111yJCeHBcA1bizdJjUFPALfhAfSRnAbJzGUtnt56A
pub const BINANCE_WALLET_ROUTER_ID: Pubkey = [
    149, 24, 7, 253, 62, 7, 233, 27, 165, 104, 5, 131, 152, 181, 62, 16, 90, 18, 243, 207, 204, 9,
    163, 144, 218, 255, 215, 253, 181, 85, 95, 163,
];
// BN111AnCthcdPVNJ6jkir9TDaS7xqXT8EhetAmYpNqFt
pub const BINANCE_WALLET_VAULT_ID: Pubkey = [
    153, 246, 16, 249, 190, 40, 38, 0, 228, 102, 38, 113, 44, 33, 99, 94, 100, 189, 115, 17, 33,
    80, 110, 163, 144, 72, 80, 126, 47, 145, 98, 55,
];
// Hph8HdfcfgEKVjc3UaVLkUZFG74r3u624VsZ8rg2aUSu
pub const LIQUID_MESH_ROUTER_ID: Pubkey = [
    249, 242, 168, 183, 77, 82, 79, 230, 178, 203, 191, 31, 80, 51, 73, 120, 213, 38, 75, 51, 96,
    214, 117, 184, 225, 109, 143, 203, 185, 79, 8, 2,
];
pub const ZERO_X_SETTLER_ID: Pubkey = [
    6, 146, 94, 30, 3, 114, 224, 219, 11, 178, 114, 145, 118, 104, 202, 119, 70, 228, 172, 208, 36,
    117, 63, 83, 200, 138, 146, 248, 180, 72, 152, 154,
];
// s7SunwrPG5SbViEKiViaDThPRJxkkTrNx2iRPN3exNC
pub const SUN_ID: Pubkey = [
    12, 214, 84, 109, 64, 174, 98, 210, 22, 184, 149, 116, 254, 253, 238, 69, 133, 107, 148, 125,
    156, 244, 181, 11, 41, 88, 187, 96, 160, 97, 92, 17,
];
// 3xCcwfFqCSggVmoyKRcDv44t8BJCu1KWYnieFjQZqMRs
pub const BITGET_WALLET_AGGREGATOR_ID: Pubkey = [
    43, 219, 116, 239, 216, 51, 106, 54, 192, 83, 99, 217, 199, 234, 25, 104, 40, 37, 52, 88, 115,
    135, 5, 42, 208, 156, 96, 26, 51, 206, 139, 210,
];

const COMPUTE_BUDGET_ID: Pubkey = [
    3, 6, 70, 111, 229, 33, 23, 50, 255, 236, 173, 186, 114, 195, 155, 231, 188, 140, 229, 187,
    197, 247, 18, 107, 44, 67, 155, 58, 64, 0, 0, 0,
];
const MEMO_V2_ID: Pubkey = [
    5, 74, 83, 90, 153, 41, 33, 6, 77, 36, 232, 113, 96, 218, 56, 124, 124, 53, 181, 221, 188, 146,
    187, 129, 228, 31, 168, 64, 65, 5, 68, 141,
];
const MEMO_V1_ID: Pubkey = [
    5, 74, 83, 80, 248, 93, 200, 130, 214, 20, 165, 86, 114, 120, 138, 41, 109, 223, 30, 171, 171,
    208, 166, 6, 120, 136, 73, 50, 244, 238, 246, 160,
];
const LIGHTHOUSE_ID: Pubkey = [
    4, 223, 173, 121, 98, 255, 177, 221, 146, 93, 10, 159, 181, 230, 208, 12, 230, 25, 91, 168,
    187, 58, 145, 253, 7, 239, 152, 96, 197, 233, 123, 184,
];
const ASSOCIATED_TOKEN_ID: Pubkey = [
    140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11, 90, 19, 153, 218,
    255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
];
const DEBRIDGE_DLN_SOURCE_ID: Pubkey = [
    13, 7, 32, 254, 68, 141, 229, 157, 136, 17, 226, 77, 109, 249, 23, 220, 141, 13, 152, 179, 146,
    221, 244, 221, 43, 98, 42, 116, 122, 96, 253, 237,
];
const DEBRIDGE_DLN_DESTINATION_ID: Pubkey = [
    9, 114, 112, 164, 97, 127, 207, 220, 85, 215, 251, 186, 218, 148, 25, 54, 48, 179, 27, 77, 76,
    119, 210, 249, 202, 194, 194, 237, 179, 193, 224, 208,
];
// DeintXmrdZFgSKzEWcJWhK793HuqnfDydAPvHmayY5Xz
const DEBRIDGE_INTENTS_ID: Pubkey = [
    187, 245, 252, 138, 159, 181, 27, 245, 248, 78, 39, 97, 46, 219, 30, 184, 131, 90, 142, 37, 35,
    46, 74, 30, 110, 51, 64, 136, 1, 42, 187, 141,
];

pub static ALLOWED_PROGRAM_IDS: [Pubkey; 24] = [
    JUPITER_V6_ID,
    OKX_DEX_V2_ID,
    TITAN_ID,
    OKX_ROUTER_ID,
    PREXGY_ID,
    DFLOW_AGGREGATOR_V4_ID,
    SECONDARY_ROUTER_ID,
    ZERO_X_SETTLER_ID,
    SUN_ID,
    BITGET_WALLET_AGGREGATOR_ID,
    COMPUTE_BUDGET_ID,
    SYSTEM_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    TOKEN_2022_PROGRAM_ID,
    MEMO_V2_ID,
    MEMO_V1_ID,
    LIGHTHOUSE_ID,
    ASSOCIATED_TOKEN_ID,
    BINANCE_WALLET_ROUTER_ID,
    BINANCE_WALLET_VAULT_ID,
    LIQUID_MESH_ROUTER_ID,
    DEBRIDGE_DLN_SOURCE_ID,
    DEBRIDGE_DLN_DESTINATION_ID,
    DEBRIDGE_INTENTS_ID,
];

pub static ROUTER_PROGRAM_IDS: [Pubkey; 13] = [
    JUPITER_V6_ID,
    OKX_DEX_V2_ID,
    TITAN_ID,
    OKX_ROUTER_ID,
    PREXGY_ID,
    DFLOW_AGGREGATOR_V4_ID,
    BINANCE_WALLET_ROUTER_ID,
    BINANCE_WALLET_VAULT_ID,
    SECONDARY_ROUTER_ID,
    LIQUID_MESH_ROUTER_ID,
    ZERO_X_SETTLER_ID,
    SUN_ID,
    BITGET_WALLET_AGGREGATOR_ID,
];

/// Peer market-maker programs recognized by routed-instruction validation.
pub static PEER_MARKET_MAKER_IDS: [Pubkey; 9] = [
    [
        122, 253, 116, 43, 39, 247, 89, 233, 198, 112, 112, 60, 211, 157, 129, 122, 160, 147,
        10, 206, 59, 82, 210, 109, 84, 160, 84, 221, 35, 135, 187, 211,
    ],
    [
        6, 211, 237, 196, 229, 115, 173, 175, 243, 229, 115, 137, 139, 84, 91, 30, 232, 210,
        141, 5, 142, 217, 78, 136, 224, 188, 179, 50, 87, 27, 234, 10,
    ],
    [
        139, 188, 19, 37, 138, 129, 117, 209, 200, 249, 24, 209, 29, 117, 187, 157, 243, 162,
        38, 11, 140, 251, 162, 230, 179, 30, 0, 33, 152, 174, 16, 230,
    ],
    [
        6, 155, 232, 110, 201, 175, 101, 235, 74, 97, 79, 217, 155, 142, 146, 84, 125, 160, 20,
        95, 171, 94, 128, 74, 219, 89, 77, 179, 231, 58, 39, 27,
    ],
    [
        6, 135, 52, 55, 180, 251, 201, 244, 4, 55, 221, 60, 230, 63, 156, 254, 140, 36, 132, 3,
        85, 147, 161, 39, 129, 186, 42, 151, 190, 52, 22, 150,
    ],
    [
        138, 176, 104, 132, 97, 57, 126, 103, 156, 196, 123, 16, 0, 158, 80, 252, 24, 74, 89,
        222, 97, 142, 204, 112, 187, 173, 151, 49, 236, 244, 174, 189,
    ],
    [
        159, 50, 218, 156, 31, 108, 18, 80, 192, 155, 115, 57, 35, 236, 195, 103, 13, 40, 99,
        231, 212, 106, 156, 28, 244, 125, 253, 187, 208, 213, 254, 87,
    ],
    [
        6, 116, 220, 237, 191, 135, 164, 147, 161, 64, 124, 253, 156, 147, 189, 9, 74, 95, 139,
        180, 252, 12, 158, 60, 214, 52, 190, 181, 5, 131, 195, 93,
    ],
    [
        6, 31, 206, 78, 120, 118, 212, 112, 246, 198, 35, 168, 147, 202, 189, 208, 21, 176, 34,
        137, 131, 224, 90, 115, 207, 60, 210, 140, 133, 97, 159, 69,
    ],
];

/// Stablecoin mints checked when classifying routes (USDC, USDT, and USD1).
pub static ROUTE_STABLECOIN_MINTS: [Pubkey; 3] = [
    [
        198, 250, 122, 243, 190, 219, 173, 58, 61, 101, 243, 106, 171, 201, 116, 49, 177, 187,
        228, 194, 210, 246, 224, 228, 124, 166, 2, 3, 69, 47, 93, 97,
    ],
    [
        206, 1, 14, 96, 175, 237, 178, 39, 23, 189, 99, 25, 47, 84, 20, 90, 63, 150, 90, 51,
        187, 130, 210, 199, 2, 158, 178, 206, 30, 32, 130, 100,
    ],
    [
        7, 7, 47, 5, 74, 180, 141, 152, 125, 164, 229, 150, 158, 99, 44, 221, 243, 141, 214,
        65, 65, 52, 157, 164, 27, 11, 105, 91, 145, 209, 92, 58,
    ],
];


/// Signing accounts recognized across all instructions when classifying a route.
pub const RECOGNIZED_ROUTE_SIGNERS: [Pubkey; 5] = [
    [
        0x8f, 0xe8, 0xf6, 0x05, 0xd6, 0xe1, 0xc7, 0x55, 0x6c, 0x37, 0x62, 0xed, 0xbd, 0xd1,
        0x6c, 0x1f, 0xa0, 0x97, 0x2d, 0x5c, 0x5b, 0xa8, 0xe3, 0x69, 0x3e, 0x1f, 0x7a, 0xb9,
        0x64, 0xed, 0x56, 0xb4,
    ],
    [
        0xbb, 0x32, 0x11, 0x71, 0x20, 0x7a, 0x2c, 0x44, 0xfc, 0xef, 0x8f, 0x89, 0x31, 0x68,
        0x29, 0x3d, 0x53, 0x2c, 0xec, 0x51, 0x80, 0x79, 0x51, 0xe4, 0x0d, 0x97, 0x57, 0x5c,
        0xe5, 0xd8, 0x9b, 0xee,
    ],
    [
        0x1b, 0xdd, 0xed, 0x37, 0x5f, 0x57, 0xc2, 0x77, 0x48, 0xf0, 0x9c, 0xe5, 0x2b, 0x7a,
        0x6e, 0x2d, 0xc4, 0x6f, 0x6c, 0x04, 0xd6, 0x1d, 0xa4, 0x7e, 0x55, 0x74, 0x7e, 0x4f,
        0xed, 0x7e, 0x79, 0x31,
    ],
    [
        0x08, 0xf1, 0x3f, 0x6f, 0xbf, 0x31, 0xe2, 0x4f, 0xe5, 0xcd, 0xb9, 0x96, 0x0f, 0x05,
        0x08, 0x31, 0x29, 0xf6, 0x4b, 0xfa, 0x5f, 0xb4, 0xff, 0x24, 0xd5, 0x7a, 0x85, 0xe5,
        0x80, 0x40, 0xf4, 0xe2,
    ],
    [
        0x06, 0x50, 0xd1, 0x2e, 0xf9, 0xbb, 0xff, 0x2b, 0x13, 0x12, 0x92, 0x1c, 0xaf, 0xae,
        0x69, 0x5d, 0x23, 0x16, 0xff, 0xe5, 0xcc, 0xac, 0x2b, 0xdc, 0xbb, 0xc6, 0x9a, 0xe9,
        0xf2, 0x3e, 0x75, 0xd1,
    ],
];

/// Single runtime registry for transaction signer classification.
pub static RECOGNIZED_ROUTE_SIGNERS_RUNTIME: [Pubkey; 5] = RECOGNIZED_ROUTE_SIGNERS;

pub static PRIORITY_ROUTE_SIGNERS: [Pubkey; 2] = [
        [
            0x95, 0x75, 0x38, 0x8a, 0xac, 0xc5, 0x4a, 0xa8, 0x85, 0x85, 0x8a, 0xa0, 0x59, 0x35, 0x46,
            0x90, 0xf9, 0x59, 0x83, 0x32, 0x72, 0x2d, 0x0f, 0x30, 0x21, 0x32, 0x1d, 0x8b, 0xcb, 0x3b,
            0x1f, 0x04,
        ],
        [
            0x2c, 0x15, 0x70, 0x0c, 0x46, 0x9d, 0xa5, 0xae, 0x43, 0x7c, 0xe9, 0x78, 0xea, 0xd6, 0x51,
            0xd2, 0x4f, 0xb6, 0x77, 0x18, 0x07, 0xdd, 0xe4, 0xd8, 0x5b, 0x60, 0x97, 0x48, 0xed, 0x34,
            0x6b, 0xf4,
        ],
    ];
