#!/usr/bin/env bash
# Prove this source is byte-for-byte the GoonFi V2 program as ORIGINALLY deployed
# (program goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE, slot 448984043, 2026-09-21).
#
# The live program was upgraded on 2026-10-06, so `solana program dump` now returns
# a newer build. The original 243,080-byte ELF survives only in the deploy
# transaction's loader buffer writes, which an archival RPC is required to read.
# Provide an Alchemy archive key (or any archival endpoint via RPC=...):
#   ALCHEMY_API_KEY=<key> ./VERIFY.sh
set -euo pipefail
cd "$(dirname "$0")"

if [ -z "${ALCHEMY_API_KEY:-}" ] && [ -z "${RPC:-}" ]; then
  echo "error: set ALCHEMY_API_KEY to an Alchemy archive key (needed to read the 2026-09-21 deploy)," >&2
  echo "       or RPC=<archival endpoint> to use another archival provider." >&2
  exit 1
fi

./build.sh
python3 verify_onchain.py onchain.bin >/dev/null
cmp onchain.bin goonfi.so
echo "IDENTICAL: this source is byte-for-byte the GoonFi V2 program deployed at slot 448984043"
