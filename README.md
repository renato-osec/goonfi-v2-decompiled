# GoonFi V2 — decompiled source

Byte-identical reconstructed Rust source of the GoonFi V2 Solana program
[`goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE`](https://solscan.io/account/goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE),
as **originally deployed at slot 448984043** (2026-09-21). The on-chain program
was upgraded on 2026-10-06, so verification pins that original deploy.

## Verify

```sh
ALCHEMY_API_KEY=<alchemy-archive-key> ./VERIFY.sh
```

Builds from source, reconstructs the original 243,080-byte program straight from
the deploy transaction's loader buffer writes, and confirms they are
byte-for-byte identical → `IDENTICAL`. Reading the 2026-09-21 deploy requires an
**archival** RPC (hence the Alchemy archive key); point `RPC=` at any archival
endpoint to use a different provider.

Build only, no RPC:

```sh
./build.sh      # -> ./goonfi.so   sha256 6a2a8670…600f767d
```

`build.sh` fetches Solana platform-tools v1.48 and compiles with
`cargo build --release --locked`.
