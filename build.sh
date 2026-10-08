#!/usr/bin/env bash
# Build the deployed GoonFi V2 program from this folder into ./goonfi.so.
# Expected sha256: 6a2a8670a56b25baac9f956c428358697b537fab7d0c3ebc13eeb880600f767d
set -euo pipefail
cd "$(dirname "$0")"

# Solana platform-tools v1.48 (rustc 1.84.1-dev, LLVM 19.1.7); set PLATFORM_TOOLS to reuse an extracted copy.
PT=${PLATFORM_TOOLS:-$PWD/platform-tools-v1.48}
[ -x "$PT/rust/bin/rustc" ] || { mkdir -p "$PT"; curl -fL https://github.com/anza-xyz/platform-tools/releases/download/v1.48/platform-tools-linux-x86_64.tar.bz2 | tar -xj -C "$PT"; }

export PATH=$PT/rust/bin:$PT/llvm/bin:$PATH RUSTC=$PT/rust/bin/rustc CC=$PT/llvm/bin/clang AR=$PT/llvm/bin/llvm-ar
export CARGO_TARGET_SBPF_SOLANA_SOLANA_RUSTFLAGS='-C lto=no -C opt-level=3 -Zremap-cwd-prefix='
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_INCREMENTAL
cargo build --release --locked --target sbpf-solana-solana
llvm-objcopy --strip-all target/sbpf-solana-solana/release/goonfi_v2.so goonfi.so
sha256sum goonfi.so
