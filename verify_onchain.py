#!/usr/bin/env python3
"""Reconstruct GoonFi V2 as it was deployed at its original slot, straight from
chain, and write the ELF to the path given as argv[1] (default: onchain.bin).

The live program was upgraded on 2026-10-06, so current state is a newer build.
The original 243,080-byte ELF only exists in the deploy transaction's loader
buffer writes (2026-09-21) — reading those needs an ARCHIVAL RPC, which is why
this requires an Alchemy archive key (or any archival endpoint via RPC=...).
Only the Python standard library is used.
"""
import json, urllib.request, urllib.error, time, hashlib, os, sys

PROGRAM     = "goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE"
PROGRAMDATA = "124gUYwjVnJQ4sJsFug9gHPzPLEtwCbAQC5LkbaDgx9s"
LOADER      = "BPFLoaderUpgradeab1e11111111111111111111111"
DEPLOY_SLOT = 448984043  # the deploy this source reconstructs (immutable anchor)
EXPECTED    = "6a2a8670a56b25baac9f956c428358697b537fab7d0c3ebc13eeb880600f767d"

KEY = os.environ.get("ALCHEMY_API_KEY")
RPC = os.environ.get("RPC") or (f"https://solana-mainnet.g.alchemy.com/v2/{KEY}" if KEY else None)
if not RPC:
    sys.exit("error: set ALCHEMY_API_KEY to an Alchemy archive key (or RPC=<archival endpoint>)")

_B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def b58decode(s):
    n = 0
    for c in s:
        n = n * 58 + _B58.index(c)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    return b"\x00" * (len(s) - len(s.lstrip("1"))) + body

def rpc(method, params, retry=8):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    for i in range(retry):
        try:
            req = urllib.request.Request(RPC, data=body, headers={"Content-Type": "application/json"})
            d = json.load(urllib.request.urlopen(req, timeout=90))
            if "error" in d:
                raise RuntimeError(d["error"])
            return d["result"]
        except urllib.error.HTTPError as e:
            if e.code == 429 and i < retry - 1:
                time.sleep(1.5 + 1.5 * i); continue
            raise
        except Exception:
            if i < retry - 1:
                time.sleep(1.5); continue
            raise

def raw_instructions(tx):
    """Yield (program_id, [account_pubkeys], data_bytes) for top-level + inner ixs."""
    msg = tx["transaction"]["message"]
    keys = list(msg["accountKeys"])
    loaded = tx.get("meta", {}).get("loadedAddresses") or {}
    keys += loaded.get("writable", []) + loaded.get("readonly", [])
    groups = [msg["instructions"]] + [g["instructions"] for g in tx.get("meta", {}).get("innerInstructions", [])]
    for group in groups:
        for ix in group:
            yield keys[ix["programIdIndex"]], [keys[a] for a in ix["accounts"]], b58decode(ix["data"])

# 1. find the upgrade/deploy at the pinned slot and read its buffer account
sys.stderr.write(f"[1/3] locating the deploy at slot {DEPLOY_SLOT} ...\n")
buffer = None
for sig in rpc("getSignaturesForAddress", [PROGRAMDATA, {"limit": 1000}]):
    if sig["slot"] != DEPLOY_SLOT:
        continue
    tx = rpc("getTransaction", [sig["signature"], {"encoding": "jsonParsed", "maxSupportedTransactionVersion": 0}])
    groups = [tx["transaction"]["message"]["instructions"]]
    groups += [g["instructions"] for g in tx.get("meta", {}).get("innerInstructions", [])]
    for group in groups:
        for ix in group:
            parsed = ix.get("parsed")
            if isinstance(parsed, dict) and parsed.get("type") in ("upgrade", "deployWithMaxDataLen"):
                info = parsed.get("info", {})
                buffer = info.get("bufferAccount") or info.get("buffer")
    if buffer:
        break
if not buffer:
    sys.exit(f"error: no upgrade/deploy at slot {DEPLOY_SLOT} (is the RPC archival?)")
sys.stderr.write(f"      buffer {buffer}\n")

# 2. reassemble the ELF from the buffer's loader Write instructions
sigs = rpc("getSignaturesForAddress", [buffer, {"limit": 1000}])
sys.stderr.write(f"[2/3] reassembling ELF from {len(sigs)} buffer transactions ...\n")
elf = bytearray()
for n, sig in enumerate(reversed(sigs)):
    tx = rpc("getTransaction", [sig["signature"], {"encoding": "json", "maxSupportedTransactionVersion": 0}])
    if not tx:
        sys.exit(f"error: transaction {sig['signature']} not retained — use an archival RPC")
    for pid, accs, data in raw_instructions(tx):
        # loader Write { offset: u32, bytes: Vec<u8> }: tag(4) offset(4) len(8) bytes
        if pid == LOADER and len(data) >= 16 and int.from_bytes(data[:4], "little") == 1 and accs and accs[0] == buffer:
            off = int.from_bytes(data[4:8], "little")
            ln = int.from_bytes(data[8:16], "little")
            chunk = data[16:16 + ln]
            if off + len(chunk) > len(elf):
                elf.extend(b"\x00" * (off + len(chunk) - len(elf)))
            elf[off:off + len(chunk)] = chunk

digest = hashlib.sha256(elf).hexdigest()
out = sys.argv[1] if len(sys.argv) > 1 else "onchain.bin"
with open(out, "wb") as f:
    f.write(elf)
sys.stderr.write(f"[3/3] on-chain ELF: {len(elf)} bytes  sha256 {digest}\n")
if digest != EXPECTED:
    sys.exit(f"error: reconstructed sha256 {digest} != expected {EXPECTED}")
print(out)
