#!/usr/bin/env python3
"""Read-only check of the mainnet claims in hrmp-issue.md (bugs B and D) on Polkadot.

Usage: python3 hrmp-verify-mainnet.py [rpc_url]   (default https://rpc.polkadot.io, needs archive state)
"""
import hashlib, json, struct, sys, time, urllib.request

URL = sys.argv[1] if len(sys.argv) > 1 else "https://rpc.polkadot.io"
DOT = 10**10

# --- storage key hashing ---------------------------------------------------------------------
P1, P2, P3, P4, P5 = 11400714785074694791, 14029467366897019727, 1609587929392839161, 9650029242287828579, 2870177450012600261
M = (1 << 64) - 1
rotl = lambda x, r: ((x << r) | (x >> (64 - r))) & M

def xxh64(data, seed=0):
	n, i = len(data), 0
	if n >= 32:
		v = [(seed + P1 + P2) & M, (seed + P2) & M, seed, (seed - P1) & M]
		while i + 32 <= n:
			for j in range(4):
				v[j] = (rotl((v[j] + struct.unpack_from('<Q', data, i + 8 * j)[0] * P2) & M, 31) * P1) & M
			i += 32
		h = (rotl(v[0], 1) + rotl(v[1], 7) + rotl(v[2], 12) + rotl(v[3], 18)) & M
		for x in v:
			h = ((h ^ ((rotl((x * P2) & M, 31) * P1) & M)) * P1 + P4) & M
	else:
		h = (seed + P5) & M
	h = (h + n) & M
	while i + 8 <= n:
		h = ((rotl(h ^ ((rotl((struct.unpack_from('<Q', data, i)[0] * P2) & M, 31) * P1) & M), 27) * P1) + P4) & M
		i += 8
	if i + 4 <= n:
		h = ((rotl(h ^ ((struct.unpack_from('<I', data, i)[0] * P1) & M), 23) * P2) + P3) & M
		i += 4
	while i < n:
		h = (rotl(h ^ ((data[i] * P5) & M), 11) * P1) & M
		i += 1
	h ^= h >> 33; h = (h * P2) & M; h ^= h >> 29; h = (h * P3) & M
	return h ^ (h >> 32)

tw128 = lambda b: struct.pack('<QQ', xxh64(b, 0), xxh64(b, 1))
tw64c = lambda b: struct.pack('<Q', xxh64(b)) + b
b2_128c = lambda b: hashlib.blake2b(b, digest_size=16).digest() + b
prefix = lambda pallet, item: tw128(pallet.encode()) + tw128(item.encode())
para_id = lambda p: struct.pack('<I', p)
para_account = lambda p: b'para' + para_id(p) + b'\0' * 24

def ss58(account, network=0):
	alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
	body = bytes([network]) + account
	data = body + hashlib.blake2b(b'SS58PRE' + body, digest_size=64).digest()[:2]
	n, out = int.from_bytes(data, 'big'), ''
	while n:
		n, r = divmod(n, 58)
		out = alphabet[r] + out
	return '1' * (len(data) - len(data.lstrip(b'\0'))) + out

# --- RPC ---------------------------------------------------------------------------------------
def rpc(method, params):
	req = urllib.request.Request(URL, json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
		{"Content-Type": "application/json", "User-Agent": "hrmp-verify/0.1"})
	for attempt in range(6):
		try:
			res = json.load(urllib.request.urlopen(req, timeout=60))
			break
		except Exception:
			if attempt == 5:
				raise
			time.sleep(2 * (attempt + 1))
	if 'error' in res:
		raise RuntimeError(res['error'])
	return res['result']

def get(key, at):
	raw = rpc("state_getStorage", ['0x' + key.hex(), at])
	return bytes.fromhex(raw[2:]) if raw else None

def block(n):
	return rpc("chain_getBlockHash", [n])

u32 = lambda b, o=0: struct.unpack_from('<I', b, o)[0]
u128 = lambda b, o=0: int.from_bytes(b[o:o + 16], 'little')
LIFECYCLE = ['Onboarding', 'Parathread', 'Parachain', 'UpgradingParathread', 'DowngradingParachain',
	'OffboardingParathread', 'OffboardingParachain']

def lifecycle(p, at):
	v = get(prefix("Paras", "ParaLifecycles") + tw64c(para_id(p)), at)
	return LIFECYCLE[v[0]] if v else 'none (removed)'

def request(sender, recipient, at):
	v = get(prefix("Hrmp", "HrmpOpenChannelRequests") + tw64c(para_id(sender) + para_id(recipient)), at)
	return None if v is None else {'confirmed': bool(v[0]), 'sender_deposit_dot': u128(v, 5) / DOT}

def open_count(p, at):
	v = get(prefix("Hrmp", "HrmpOpenChannelRequestCount") + tw64c(para_id(p)), at)
	return u32(v) if v else 0

def pending_requests_from(p, at):
	pre = '0x' + prefix("Hrmp", "HrmpOpenChannelRequests").hex()
	keys, start = [], None
	while True:
		page = rpc("state_getKeysPaged", [pre, 1000, start, at])
		keys += page
		if len(page) < 1000:
			break
		start = page[-1]
	return [k for k in keys if u32(bytes.fromhex(k[2:]), 40) == p]

def reserved(p, at):
	v = get(prefix("System", "Account") + b2_128c(para_account(p)), at)
	return u128(v, 32) / DOT if v else 0

# --- checks ------------------------------------------------------------------------------------
head = rpc("chain_getFinalizedHead", [])
print(f"endpoint {URL}, finalized head {head}\n")

print("Para sovereign accounts (paste into Polkadot.js > Accounts / Chain state > system.account):")
for p in (2004, 2012, 2008, 2025):
	print(f"  para {p}: {ss58(para_account(p))}")

REMOVAL = 32_492_253  # para 2004 goes from OffboardingParathread to removed
before, after = block(REMOVAL - 1), block(REMOVAL)
print(f"\nBlock {REMOVAL - 1}: {before}\nBlock {REMOVAL}: {after}")

print(f"\nPara 2004 lifecycle: {lifecycle(2004, before)} -> {lifecycle(2004, after)}")

print("\nBug B: requests to 2004 are removed but the senders' counters are not decremented")
for sender in (2008, 2025):
	print(f"  {sender}->2004 request: {request(sender, 2004, before)} -> {request(sender, 2004, after)}")
	print(f"  {sender} HrmpOpenChannelRequestCount: {open_count(sender, before)} -> {open_count(sender, after)}"
		f"; today {open_count(sender, head)} with {len(pending_requests_from(sender, head))} pending requests")

print("\nBug D: 2004's own pending requests are removed but their deposits stay reserved")
for recipient in (2086, 3344, 3345, 3369, 3388):
	print(f"  2004->{recipient}: {request(2004, recipient, before)} -> {request(2004, recipient, after)}")
print(f"  2004 reserved: {reserved(2004, before)} DOT -> {reserved(2004, after)} DOT; today {reserved(2004, head)} DOT")
print(f"  2012 reserved today: {reserved(2012, head)} DOT (lifecycle: {lifecycle(2012, head)})")
