# GoonFi V2 

Byte-identical reconstructed Rust source of the GoonFi V2 Solana program
[`goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE`](https://solscan.io/account/goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE),
as **originally deployed at slot 448984043**. The on-chain program
was upgraded on 2026-10-06 to fix the reported bugs

## Verify

```sh
ALCHEMY_API_KEY=<alchemy-archive-key> ./VERIFY.sh
```

Builds from source, reconstructs the program straight from
the deploy transaction's loader buffer writes, and confirms they match beyond padding 

Build only, no RPC:

```sh
./build.sh      # -> ./goonfi.so   sha256 6a2a8670…600f767d
```

`build.sh` fetches Solana platform-tools v1.48 and compiles with
`cargo build --release --locked`.
