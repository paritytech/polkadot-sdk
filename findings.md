# paritytech/polkadot-sdk #11662
  
**Repository:** paritytech/polkadot-sdk  
**PR:** [#11662](https://github.com/paritytech/polkadot-sdk/pull/11662) — Hand-off protocol  
**Commit:** `6dceb293782d`  
**Crates:** `pallet-transaction-storage`, `polkadot-omni-node-lib`, `sc-hop`, `sp-hop`
---

**Findings:** 7 (open)

---

## Unbounded recipient list enables node memory exhaustion bypassing pool capacity limits

**Severity:** High  
**File:** `substrate/client/hop/src/pool.rs:226-253`  
**Category:** Denial of Service  
**Phase:** Open  
**Agent verdict:** high  
**Human verdict:** high

### Report

### Description

The `HopDataPool::insert` function validates that the recipients list is non-empty (`recipients.is_empty()`) but imposes no upper bound on `recipients.len()`. Each recipient is a `MultiSigner` enum (33-36 bytes SCALE-encoded), and the full `Vec<MultiSigner>` plus a corresponding `Vec<bool>` for claimed status are stored in the in-memory `index` HashMap as part of `HopEntryMeta`. Critically, pool capacity tracking (`current_size`) only accounts for `data.len()` -- metadata size is entirely uncounted.

An attacker with a valid Bulletin Chain authorization can submit a 1-byte data payload with millions of recipient keys, consuming tens of megabytes of RAM per entry while the pool capacity counter only registers 1 byte.

### Vulnerable Code

[`substrate/client/hop/src/pool.rs:226-232`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L226-L232)

```rust
pub fn insert(
    &self,
    data: Vec<u8>,
    current_block: HopBlockNumber,
    recipients: Vec<MultiSigner>,
    sender_id: SenderId,
) -> Result<HopHash, HopError> {
    // Validate recipients
    if recipients.is_empty() {
        return Err(HopError::NoRecipients);
    }
    // NO upper bound check on recipients.len()
```

The metadata struct stored per entry in RAM ([`substrate/client/hop/src/types.rs:28-44`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/types.rs#L28-L44)):

```rust
pub struct HopEntryMeta {
    pub added_at: HopBlockNumber,
    pub expires_at: HopBlockNumber,
    pub size: u64,
    pub recipients: Vec<MultiSigner>,  // unbounded
    pub claimed: Vec<bool>,            // unbounded, same length
    pub sender_id: SenderId,
    pub promoted: bool,
}
```

Pool capacity only tracks data size ([`substrate/client/hop/src/pool.rs:243-252`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L243-L252)):

```rust
let data_len = data.len() as u64;
// ...
let prev_size = self.current_size.fetch_add(data_len, Ordering::Relaxed);
```

The RPC layer also performs no length check on recipients ([`substrate/client/hop/src/rpc.rs:151-157`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L151-L157)):

```rust
let recipient_keys: Vec<MultiSigner> = recipients
    .into_iter()
    .map(|r| {
        MultiSigner::decode(&mut &r.0[..])
            .map_err(|_| ErrorObjectOwned::from(HopError::InvalidRecipientKey))
    })
    .collect::<RpcResult<Vec<_>>>()?;
```

### Exploit Scenario

1. Attacker obtains a valid Bulletin Chain authorization (required for `hop_submit`).
2. Attacker calls `hop_submit` with `data = [0x00]` (1 byte) and 1,000,000 SCALE-encoded `MultiSigner` keys as recipients.
3. Pool capacity check passes since `data.len() = 1`, well under any quota.
4. `HopEntryMeta` is inserted into the in-memory `index` HashMap, consuming approximately 34 MB of RAM (33 bytes per `MultiSigner` + 1 byte per `bool` in `claimed`).
5. The pool reports only 1 byte of data usage.
6. Repeating with different data bytes (to avoid the duplicate check) allows unbounded RAM growth without triggering pool capacity limits.
7. With 1000 such entries, the node consumes approximately 34 GB of RAM, likely causing an OOM crash.

### Impact

Node memory exhaustion leading to crash or severe performance degradation. The fundamental invariant that pool capacity limits protect node resources is violated because metadata size is not accounted for.

### Recommendation

1. Add a `MAX_RECIPIENTS` constant (e.g., 64 or 256) and enforce it in both `pool.rs::insert()` and `rpc.rs::submit()` before processing.
2. Include estimated metadata size in pool capacity accounting, not just `data.len()`.

### Verification

**Verdict:** partially-valid

## Verification Record
### Verdict: Partially Valid
The core vulnerability mechanism -- unbounded recipient lists with metadata excluded from pool capacity tracking -- is confirmed by all three independent analyses and represents a real memory amplification attack vector. However, the finding's specific exploit parameters (1,000,000 recipients per request, 34 MB per entry, 34 GB total from 1,000 entries) are materially overstated because the jsonrpsee 15 MB request body limit caps recipients at approximately 200,000 per request and metadata at approximately 7 MB per entry.

### Red Team Summary
- Confirmed no upper bound on `recipients.len()` anywhere in the code path (pool.rs, rpc.rs).
- Confirmed `HopEntryMeta` stores unbounded `Vec<MultiSigner>` and `Vec<bool>` in memory.
- Confirmed pool capacity (`current_size`) and per-user quota both track only `data.len()`, completely ignoring metadata size.
- Acknowledged the 15 MB JSON-RPC limit but argued ~200K recipients still yields ~7-8.5 MB of untracked metadata per entry, an amplification of ~7,000,000x against what the pool counter registers (1 byte).
- Noted that deduplication is trivially bypassed by varying the 1-byte data payload.
- Noted that RPC rate limiting is optional and not enabled by default.
- Identified disk amplification through .meta files as an additional vector.

### Blue Team Summary
- Argued that the 15 MB JSON-RPC body limit makes 1,000,000 recipients physically impossible per request.
- Calculated ~7 MB metadata per entry (200K recipients x ~35 bytes), not 34 MB.
- Argued content-addressed deduplication prevents repeating identical payloads (though Red Team showed this is trivially bypassed).
- Pointed to authorization/signature verification as barriers, connection limits (100 max), and optional rate limiting.
- Characterized the amplification as "modest" by framing it as 15 MB request to 7 MB metadata -- however, this framing mischaracterizes the vulnerability since the pool counter registers only 1 byte, not 15 MB.
- Concluded the finding is invalid because the specific numbers are wrong.

### Neutral Analysis Summary
- Verified all line numbers with minor corrections: insert function at 226-232, is_empty check at 234-236, fetch_add at 249, capacity check at 250-253, HopEntryMeta at 28-44, RPC decoding at 151-157.
- Confirmed complete call path: RPC submit -> pool.insert -> HopEntryMeta::new -> index.insert + disk write.
- Confirmed no upper bound on recipients.len() exists anywhere in the code path.
- Confirmed pool capacity and per-user quota both use data.len() exclusively.
- Measured MultiSigner enum: Ed25519/Sr25519 at 33 bytes (32 + discriminant), Ecdsa at 34 bytes (33 + discriminant). Claimed vector at 1 byte per recipient.
- Confirmed disk recovery also uses meta.size (= data size) for current_size, not including metadata memory.

### Evidence Assessment
The strongest evidence supports a **partially-valid** verdict for the following reasons:

**The vulnerability mechanism is real and confirmed.** All three analysts agree on the fundamental facts: (1) no upper bound on recipients, (2) pool capacity ignores metadata size, (3) per-user quota also ignores metadata size. These are not disputed by any party.

**The Blue Team's "modest amplification" argument is the weakest point of their analysis.** They frame the ratio as "15 MB request to 7 MB metadata," implying the amplification is less than 1:1. This is misleading because the pool capacity counter -- the mechanism meant to prevent resource exhaustion -- registers only `data.len()` (1 byte in the exploit), not the network request size. The true amplification ratio against pool tracking is approximately 7,000,000:1 (7 MB metadata per 1 byte tracked). The pool's capacity limit is thus rendered meaningless as a protective mechanism.

**The Blue Team's strongest argument -- that the finding's numbers are materially wrong -- is correct.** The finding claims 1,000,000 recipients per request and 34 MB per entry, but the physical limit is ~200,000 recipients and ~7 MB per entry. This is roughly a 5x overstatement. The finding's headline claim of "34 GB from 1000 entries" should be approximately 7 GB from 1000 entries. While still a significant DoS vector, the reduced numbers lower the practical severity.

**Feasibility of exploitation.** An attacker needs valid Bulletin Chain authorization (standard user-level, not admin). Without rate limiting (the default), sending thousands of requests to accumulate gigabytes of untracked memory is feasible. The deduplication bypass (varying the data byte) is trivial. Connection limits of 100 concurrent connections do not prevent sequential exploitation.

**The finding remains a genuine vulnerability** because the pool capacity tracking -- which exists precisely to prevent resource exhaustion -- is completely ineffective against metadata-based memory growth. This is a design flaw that should be fixed regardless of the exact amplification numbers. However, the severity should be moderated from "high" to reflect the corrected impact parameters.

### Permalink Corrections
- Original insert function: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L226-L234`
- Corrected insert function: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L226-L232`
- Original HopEntryMeta: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/types.rs#L30-L44`
- Corrected HopEntryMeta: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/types.rs#L28-L44`
- Original RPC decoding: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L151-L158`
- Corrected RPC decoding: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L151-L157`
- Capacity tracking (unchanged): `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L243-L253`

---

## O(N) signature verification over unbounded recipient list enables CPU exhaustion

**Severity:** Medium  
**File:** `substrate/client/hop/src/pool.rs:356-372`  
**Category:** Denial of Service  
**Phase:** Open  
**Agent verdict:** medium  
**Human verdict:** medium

### Report

### Description

The `find_recipient` function performs a linear scan over all recipients of a pool entry, verifying a cryptographic signature against each recipient's public key until a match is found. Combined with the unbounded recipient list (FINDING_001), this allows an attacker to force O(N) expensive signature verifications per `hop_claim` or `hop_ack` call. When called with an invalid signature, all N verifications run before returning `NotRecipient`.

Furthermore, `hop_claim` and `hop_ack` do not require on-chain authorization -- only a signature matching a recipient key. Any RPC client who knows an entry hash can trigger this expensive computation.

### Vulnerable Code

[`substrate/client/hop/src/pool.rs:356-369`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L356-L369)

```rust
fn find_recipient(
    meta: &HopEntryMeta,
    hash: &HopHash,
    signature: &[u8],
) -> Result<usize, HopError> {
    let multi_sig =
        MultiSignature::decode(&mut &signature[..]).map_err(|_| HopError::InvalidSignature)?;

    meta.recipients
        .iter()
        .enumerate()
        .find_map(|(i, signer)| {
            let account_id = signer.clone().into_account();
            if multi_sig.verify(hash.as_bytes(), &account_id) { Some(i) } else { None }
        })
        .ok_or(HopError::NotRecipient)
}
```

This is called from `claim` ([`pool.rs:381`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L381)) and `ack` ([`pool.rs:410`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L410)).

The RPC endpoints pass through without authorization ([`substrate/client/hop/src/rpc.rs:199-210`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L199-L210)):

```rust
fn claim(&self, hash: Bytes, signature: Bytes) -> RpcResult<Bytes> {
    let hash = Self::bytes_to_hash(hash)?;
    let data = self.pool.claim(&hash, &signature.0)?;
    Ok(Bytes(data))
}

fn ack(&self, hash: Bytes, signature: Bytes) -> RpcResult<()> {
    let hash = Self::bytes_to_hash(hash)?;
    self.pool.ack(&hash, &signature.0)?;
    Ok(())
}
```

### Exploit Scenario

1. An authorized attacker (or a legitimate user as setup) submits an entry with 100,000 recipients (permitted due to FINDING_001).
2. An unauthenticated attacker who observes or guesses the 32-byte content hash calls `hop_claim` with a deliberately invalid signature.
3. The `find_recipient` function performs 100,000 `MultiSignature::verify()` operations (each involving elliptic curve math, especially expensive for sr25519/ecdsa) before returning `NotRecipient`.
4. Repeating this call at high frequency exhausts node CPU, degrading block production and other RPC services.
5. No rate limiting exists at the HOP RPC layer; only the optional global `--rpc-rate-limit` provides any throttling.

### Impact

CPU exhaustion on the collator node. The node's ability to process blocks and serve other RPC requests is degraded. The attack requires no on-chain authorization for the claim/ack calls, only knowledge of an entry hash.

### Recommendation

1. Fix FINDING_001 first by capping `recipients.len()` to a reasonable bound (e.g., 256). This bounds the signature verification scan.
2. Consider adding HOP-specific rate limiting for `hop_claim` and `hop_ack`, or requiring a proof-of-work token to prevent trivial flooding.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)
### Verdict: Potential
Code location confirmed and all factual claims verified. The `find_recipient` function performs O(N) signature verifications over an unbounded recipients list, callable via unauthenticated RPC endpoints.

### Neutral Analysis Summary
- `find_recipient` verified at pool.rs:356-372 (finding claimed 356-369; actual function extends to line 372). Iterates `meta.recipients` calling `multi_sig.verify()` on each.
- Called from `claim` (pool.rs:384) and `ack` (pool.rs:415) — both confirmed at claimed locations.
- RPC methods `hop_claim` (rpc.rs:198-202) and `hop_ack` (rpc.rs:204-208) confirmed to have no authorization checks. Contrast with `hop_submit` which checks `is_account_authorized()`.
- No upper bound on `recipients.len()` — only an `is_empty()` check exists at pool.rs:234. `HopEntryMeta.recipients` is `Vec<MultiSigner>` with no bounded wrapper.
- No HOP-specific rate limiting exists; only the node-global `--rpc-rate-limit` flag applies externally.

### Permalink Corrections
- Original: https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L356-L369
- Corrected: https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L356-L372

---

## Promotion task is never instantiated; data silently expires instead of being promoted on-chain

**Severity:** Medium  
**File:** `cumulus/polkadot-omni-node/lib/src/common/spec.rs:433-455`  
**Category:** Logic Error  
**Phase:** Open  
**Agent verdict:** medium  
**Human verdict:** medium

### Report

### Description

The PR description states that HOP provides a "best-effort fallback" where unclaimed data is automatically promoted to on-chain storage before expiry. The `HopMaintenanceTask` struct in `promotion.rs` implements this logic, combining promotion of near-expiry entries with cleanup of expired entries. However, the integration code in both `spec.rs` and `aura.rs` only spawns a bare `hop-cleanup` loop that calls `cleanup_expired()` -- it never instantiates `HopMaintenanceTask` or calls `try_build_promoter`. The promotion path is dead code.

### Vulnerable Code

Integration code spawns cleanup-only ([`cumulus/polkadot-omni-node/lib/src/common/spec.rs` patch lines 433-457](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/cumulus/polkadot-omni-node/lib/src/common/spec.rs#L433-L457)):

```rust
if let Some(ref pool) = hop_pool {
    let task_pool = pool.clone();
    let task_client = client.clone();
    let check_interval = node_extra_args.hop.check_interval;
    task_manager.spawn_handle().spawn("hop-cleanup", None, async move {
        loop {
            futures_timer::Delay::new(Duration::from_secs(check_interval)).await;
            let block = task_client.info().best_number.saturated_into::<u32>();
            let freed = task_pool.cleanup_expired(block);
            if freed > 0 {
                log::info!(
                    target: "hop",
                    "Cleaned up expired HOP entries, freed {} bytes",
                    freed,
                );
            }
        }
    });
}
```

The same pattern is repeated in `aura.rs` for dev nodes.

The `HopMaintenanceTask` is defined but never used ([`substrate/client/hop/src/promotion.rs:130-156`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/promotion.rs#L130-L156)):

```rust
pub struct HopMaintenanceTask {
    hop_pool: Arc<HopDataPool>,
    promoter: Option<Arc<dyn HopPromoter>>,
    buffer_blocks: u32,
    check_interval_secs: u64,
    best_block: Arc<dyn Fn() -> u32 + Send + Sync>,
}
// ...
pub async fn run(self) {
    loop {
        futures_timer::Delay::new(Duration::from_secs(self.check_interval_secs)).await;
        self.tick();
    }
}
```

`try_build_promoter` is also exported from `lib.rs` but never called in any integration code.

### Impact

Data submitted to the HOP pool that is not claimed by recipients within the retention window (default 24 hours) is silently deleted instead of being promoted to permanent on-chain storage. Users who rely on the promotion guarantee as a data durability safety net will experience silent data loss. This contradicts the PR description which states: "If the receiver never shows up within the retention window (default 24 h), the collator attempts to promote the data to on-chain storage as a best-effort fallback."

### Recommendation

Replace the inline cleanup loop with a properly instantiated `HopMaintenanceTask` that includes promotion logic. Wire up `try_build_promoter` to detect runtime API support and enable promotion when available.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)
### Verdict: Potential
Code location verified and finding claims confirmed: the HOP integration code only spawns a cleanup loop, never instantiating the `HopMaintenanceTask` that implements the promotion path described in the PR.

### Neutral Analysis Summary

- **spec.rs cleanup loop (lines 433-455):** Confirmed. The `start_node` function spawns a `"hop-cleanup"` task that only calls `cleanup_expired()` in a loop. No promotion logic is wired.
- **aura.rs cleanup loop (lines 420-442):** Confirmed identical cleanup-only pattern for dev nodes.
- **HopMaintenanceTask (promotion.rs lines 129-135, impl 137-211):** Struct and its `run`/`tick` methods exist and contain full promotion + cleanup logic, including calls to `get_promotable()`, `promoter.promote()`, and `mark_promoted()`. Only used in unit tests within `promotion.rs`.
- **try_build_promoter (promotion.rs lines 90-125):** Function exists and is re-exported from `lib.rs` line 105. Never imported or called in any integration code under `cumulus/`.
- **Integration imports:** `cumulus/` only imports `HopDataPool` and RPC types from `sc_hop` — no promotion-related types.
- **Pool methods `get_promotable` and `mark_promoted`:** Public on `HopDataPool` but only called from `HopMaintenanceTask::tick` and unit tests — never from integration code.

### Permalink Corrections
- Original: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/cumulus/polkadot-omni-node/lib/src/common/spec.rs#L433-L457`
- Corrected: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/cumulus/polkadot-omni-node/lib/src/common/spec.rs#L433-L455`

---

## Single authorized user can monopolize the entire pool, denying service to all others

**Severity:** Medium  
**File:** `substrate/client/hop/src/pool.rs:258-273`  
**Category:** Denial of Service  
**Phase:** Open  
**Agent verdict:** medium  
**Human verdict:** medium

### Report

### Description

The per-user quota is dynamically calculated as `max_size / active_users`. When only one user has data in the pool, their limit is `max_size / 1 = max_size` (the full pool, 10 GiB by default). A single authorized user can fill the entire pool before any other user submits. Once full, the global capacity check (`current_size + data_len > max_size`) rejects all new submissions before the per-user quota is even evaluated.

The dynamic quota does not retroactively reclaim space. If user A fills 10 GiB when they are the only user, and user B joins, user B's quota would be 5 GiB -- but the pool is already full, so they are rejected at the capacity check.

### Vulnerable Code

[`substrate/client/hop/src/pool.rs:259-270`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L259-L270)

```rust
{
    let usage_map = self.user_usage.read();
    let current_usage = usage_map.get(&sender_id).copied().unwrap_or(0);
    let is_new_user = current_usage == 0;
    let active_users =
        if is_new_user { usage_map.len() as u64 + 1 } else { usage_map.len() as u64 };
    let per_user_limit = self.max_size / active_users.max(1);

    if current_usage + data_len > per_user_limit {
        self.current_size.fetch_sub(data_len, Ordering::Relaxed);
        return Err(HopError::UserQuotaExceeded {
            used: current_usage,
            limit: per_user_limit,
        });
    }
}
```

The global capacity check happens first ([`pool.rs:249-252`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L249-L252)):

```rust
let prev_size = self.current_size.fetch_add(data_len, Ordering::Relaxed);
if prev_size + data_len > self.max_size {
    self.current_size.fetch_sub(data_len, Ordering::Relaxed);
    return Err(HopError::PoolFull(prev_size, self.max_size));
}
```

### Exploit Scenario

1. Attacker obtains a valid Bulletin Chain authorization.
2. Attacker submits many 8 MiB blobs (the maximum per-entry size). With 10 GiB default pool, this takes ~1280 entries.
3. The per-user limit check passes because the attacker is the only user: `per_user_limit = 10 GiB / 1 = 10 GiB`.
4. Pool is now full. Any new user's submission fails with `PoolFull` at the global capacity check, before the quota system even runs.
5. The attacker's data persists for the full retention period (default 14400 blocks / ~24 hours), during which no other user can submit anything.

### Impact

Complete denial of service for all other authorized users of the HOP pool for up to 24 hours per attack cycle. The attacker only needs one valid authorization to monopolize the entire pool.

### Recommendation

Implement a fixed per-user maximum quota independent of the number of active users (e.g., `max_size / MAX_EXPECTED_USERS` or a configurable absolute cap per account). Consider implementing an eviction policy where new submissions can evict older entries from over-quota users.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)

### Verdict: Potential

Code location and logic verified. The dynamic per-user quota (`max_size / active_users`) allows a single authorized user to consume the full pool capacity when no other users have data. No eviction or rebalancing mechanism exists to reclaim space.

### Neutral Analysis Summary

#### Code Location Confirmed
- `HopDataPool::insert` exists at lines 226-336 of `substrate/client/hop/src/pool.rs`.
- Global capacity check at lines 249-253 executes **before** the per-user quota check at lines 258-273.
- Code content matches the finding's quotations exactly.

#### Insert Flow (Verified Order of Checks)
1. Recipient validation (line 234)
2. Empty data check (line 239)
3. Per-entry size check (lines 243-246) — max 8 MiB per entry
4. **Global capacity check** (lines 249-253) — atomic `fetch_add`, rejects with `PoolFull` if over `max_size`
5. **Per-user quota check** (lines 258-273) — `per_user_limit = max_size / active_users.max(1)`
6. Duplicate check, disk write, usage update

#### Dynamic Quota Behavior
- When one user exists: `per_user_limit = max_size / 1 = max_size` (full pool)
- When a second user arrives: their limit would be `max_size / 2`, but the pool is already full so `PoolFull` fires first
- No retroactive reclamation or eviction exists

#### Mitigations Searched — None Found
- **Fixed per-user cap**: Not present
- **Eviction policy**: Not present
- **Rate limiting**: Only global `--rpc-rate-limit`, no HOP-specific limit
- **Authorization**: Required (Bulletin Chain), but does not limit volume per authorized account
- **Cleanup**: Runs on 1-hour default interval; retention is ~24 hours (14,400 blocks)

#### Defaults
- Pool size: 10 GiB (`DEFAULT_MAX_POOL_SIZE`)
- Max entry size: 8 MiB (`MAX_DATA_SIZE`)
- Entries to fill pool: ~1,280
- Retention: 14,400 blocks (~24 hours)
- Cleanup interval: 3,600 seconds (1 hour)

### Permalink Corrections
- Original: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L259-L270`
- Corrected: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L258-L273`

---

## Missing domain separation in signatures allows cross-context replay between submit and claim/ack

**Severity:** Low  
**File:** `substrate/client/hop/src/rpc.rs:160-164`  
**Category:** Cryptography  
**Phase:** Open  
**Agent verdict:** medium  
**Human verdict:** low

### Report

### Description

The HOP protocol signs and verifies the same 32-byte value (`blake2_256(data)`) across different operation contexts without any domain separator. In `submit`, the signer signs `blake2_256(data)`. In `claim` and `ack`, the recipient signs the same content hash. There is no context prefix (e.g., `b"hop-submit:"`, `b"hop-claim:"`) to distinguish the purpose of the signature.

This means a `submit` signature can be replayed as a `claim`/`ack` signature if the submitter is also listed as a recipient, since the signed message is identical.

### Vulnerable Code

Submit signature verification ([`substrate/client/hop/src/rpc.rs:161-164`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L161-L164)):

```rust
let hash = H256(blake2_256(&data.0));
let account_id: AccountId32 = signer.into_account();
if !multi_sig.verify(hash.as_bytes(), &account_id) {
    return Err(ErrorObjectOwned::from(HopError::InvalidSignature));
}
```

Claim/ack signature verification ([`substrate/client/hop/src/pool.rs:356-369`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L356-L369)):

```rust
fn find_recipient(
    meta: &HopEntryMeta,
    hash: &HopHash,
    signature: &[u8],
) -> Result<usize, HopError> {
    let multi_sig =
        MultiSignature::decode(&mut &signature[..]).map_err(|_| HopError::InvalidSignature)?;
    meta.recipients
        .iter()
        .enumerate()
        .find_map(|(i, signer)| {
            let account_id = signer.clone().into_account();
            if multi_sig.verify(hash.as_bytes(), &account_id) { Some(i) } else { None }
        })
        .ok_or(HopError::NotRecipient)
}
```

Both verify `signature` over the same `hash.as_bytes()` (the content hash) with no domain context.

### Exploit Scenario

1. Alice submits data to HOP, listing herself as both the signer and a recipient. Her `submit` call includes a signature over `blake2_256(data)`.
2. An attacker observing the RPC call (e.g., on a public JSON-RPC endpoint or via network sniffing) captures Alice's submit signature.
3. The attacker calls `hop_ack(hash, alice_submit_signature)`. Since the signed message is identical (`blake2_256(data)` = the content hash) and Alice is a recipient, the signature verifies successfully.
4. Alice is marked as having acknowledged. If she was the only recipient, the entry is deleted before she retrieves it.
5. Additionally, signatures produced for HOP could potentially be replayed in other systems that sign raw 32-byte blake2 hashes, or vice versa.

### Impact

Unauthorized acknowledgment and premature deletion of pool entries. Data loss for intended recipients who have not yet claimed. The cross-protocol replay risk also exists for any system that signs raw 32-byte values without domain separation.

### Recommendation

Add domain-specific prefixes to the signed message for each operation:
- Submit: sign `b"hop-submit:" || blake2_256(data)`
- Claim: sign `b"hop-claim:" || content_hash`
- Ack: sign `b"hop-ack:" || content_hash`

This ensures signatures are not interchangeable across contexts or other protocols.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)

### Verdict: Potential

Code exists at claimed locations and all three HOP operations verify signatures over the identical 32-byte `blake2_256(data)` value with no domain separator, confirming the cross-context replay risk described in the finding.

### Neutral Analysis Summary

- **Submit** (`rpc.rs:160-164`): computes `H256(blake2_256(&data.0))` and verifies `multi_sig.verify(hash.as_bytes(), &account_id)` against the signer.
- **Claim** (`rpc.rs:198-201` → `pool.rs:384` → `pool.rs:356-372`): calls `find_recipient` which verifies `multi_sig.verify(hash.as_bytes(), &account_id)` against each recipient.
- **Ack** (`rpc.rs:204-208` → `pool.rs:415` → `pool.rs:356-372`): same `find_recipient` verification path as claim.
- All three operations sign/verify the same `hash.as_bytes()` — no domain prefix, context tag, nonce, or any other differentiator.
- No mechanism prevents a signer from also being a recipient (confirmed by existing test `claim_and_ack_through_rpc` at `rpc.rs:420-446`).
- Searched the entire `substrate/client/hop/src/` directory for "domain", "prefix", "separator", "context" — zero matches.

### Permalink Corrections

- Original: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L161-L164`
- Corrected: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L160-L164`

#### Additional Verified Permalinks

- `find_recipient` (pool.rs:356-372): `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L356-L372`
- Test confirming cross-context usage (rpc.rs:420-446): `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/rpc.rs#L420-L446`

---

## Per-user quota TOCTOU race allows concurrent inserts to bypass fair-share limits

**Severity:** Low  
**File:** `substrate/client/hop/src/pool.rs:249-273`  
**Category:** Race Condition  
**Phase:** Open  
**Agent verdict:** low  
**Human verdict:** low

### Report

### Description

The `insert` function has a TOCTOU (time-of-check-time-of-use) gap between the per-user quota check and the usage update. Pool capacity is reserved atomically via `fetch_add`, but the per-user quota is checked under a **read lock** on `user_usage`, while the actual usage increment happens later under a **write lock** (after disk I/O and index insertion). Two concurrent `insert` calls from the same user can both read `current_usage = 0`, both pass the quota check, and both succeed, effectively doubling the user's allowed allocation.

The code comments acknowledge this behavior: "two concurrent inserts from the same user could both pass this check, but they cannot exceed max_size."

### Vulnerable Code

[`substrate/client/hop/src/pool.rs:249-273`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L249-L273)

```rust
// Eagerly reserve pool capacity. Roll back on any subsequent failure.
let prev_size = self.current_size.fetch_add(data_len, Ordering::Relaxed);
if prev_size + data_len > self.max_size {
    self.current_size.fetch_sub(data_len, Ordering::Relaxed);
    return Err(HopError::PoolFull(prev_size, self.max_size));
}

// Per-user quota enforcement (soft limit — ...)
{
    let usage_map = self.user_usage.read();  // READ lock
    let current_usage = usage_map.get(&sender_id).copied().unwrap_or(0);
    // ... quota check ...
}
// ... disk I/O happens here with NO lock ...
// ... then later:
*self.user_usage.write().entry(sender_id).or_insert(0) += data_len;  // WRITE lock
```

The gap between the read-lock quota check (line 259) and the write-lock usage update (line 325) spans disk I/O operations, creating a wide window for concurrent exploits.

### Impact

A user can temporarily exceed their fair-share quota by issuing concurrent `submit` calls. The global pool capacity limit (`max_size`) is never violated, so this is bounded. The practical impact is that under concurrent submissions, the fair-share allocation becomes unreliable, allowing one user to squeeze out others more than intended.

### Recommendation

Either hold the write lock on `user_usage` throughout the entire insert operation, or accept the soft-limit nature of the quota and document it clearly. Given that global capacity is the hard safety net, the current design is acceptable if the quota is documented as best-effort.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)

### Verdict: Potential

TOCTOU race condition verified at claimed location. The per-user quota is checked under a read lock (L259) with the usage update under a write lock at L325, spanning ~66 lines including two disk I/O operations. The code authors acknowledge and accept this as a soft limit.

### Neutral Analysis Summary

- **File exists**: `substrate/client/hop/src/pool.rs` confirmed at commit `6dceb293782d4944252d1d264bf99da378bbcfa2`
- **Function**: `HopDataPool::insert` spans lines 226-336
- **Global capacity reservation** (`fetch_add`): Line 249 — atomically enforced, never bypassable
- **Per-user quota read lock**: Lines 258-273 — quota checked under `user_usage.read()`
- **TOCTOU window** (lines 273-325): Includes blake2 hashing, duplicate check, SCALE encoding, two atomic disk writes (blob at L297, meta at L304), and index insertion
- **Per-user quota write lock**: Line 325 — `user_usage.write()` increments usage
- **Code comment** (lines 255-257): Explicitly acknowledges the race: "two concurrent inserts from the same user could both pass this check, but they cannot exceed max_size"
- **Test coverage**: `test_concurrent_inserts_respect_user_quota` (lines 1159-1193) exercises the race and only asserts the hard capacity limit, confirming best-effort semantics
- **Line number correction**: Finding claimed L249-L270; actual quota block ends at L273 (off by 3 lines)

### Permalink Corrections

- Original: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L249-L270`
- Corrected: `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L249-L273`

---

## Duplicate recipients in submission prevent full acknowledgment, causing entries to persist until expiry

**Severity:** Low  
**File:** `substrate/client/hop/src/pool.rs:234-236`  
**Category:** Input Validation  
**Phase:** Open  
**Agent verdict:** low  
**Human verdict:** low

### Report

### Description

Neither the RPC layer nor the pool layer validates that the recipients list contains unique `MultiSigner` values. An attacker (or careless user) can submit the same recipient key N times. The `find_recipient` function uses `find_map` which always matches the first occurrence. Consequently:

1. `claimed` has N entries for the same key, but only the first can ever be set to `true` via `ack`.
2. The remaining N-1 duplicates are never acknowledged.
3. The check `meta.claimed.iter().all(|&c| c)` (at pool.rs line 436) will never return `true`, so the entry is never auto-deleted via the ack path.
4. The entry persists until the expiry cleanup removes it.

### Vulnerable Code

[`substrate/client/hop/src/pool.rs:234-236`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L234-L236) -- only checks emptiness:

```rust
if recipients.is_empty() {
    return Err(HopError::NoRecipients);
}
```

No deduplication or uniqueness check follows.

[`substrate/client/hop/src/pool.rs:364-371`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L364-L371) -- always finds the first match:

```rust
meta.recipients
    .iter()
    .enumerate()
    .find_map(|(i, signer)| {
        let account_id = signer.clone().into_account();
        if multi_sig.verify(hash.as_bytes(), &account_id) { Some(i) } else { None }
    })
```

[`substrate/client/hop/src/pool.rs:436`](https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L436) -- requires all claimed:

```rust
if meta.claimed.iter().all(|&c| c) {
```

### Impact

Entries with duplicate recipients become permanently un-acknowledgeable and persist until expiry cleanup. This wastes pool resources (both disk and memory) for the full retention period. When combined with FINDING_001 (no limit on recipient count), a large number of duplicates amplifies the resource waste.

### Recommendation

Either deduplicate the recipients list in `pool.rs::insert()` (or `rpc.rs::submit()`), or reject submissions containing duplicate `MultiSigner` values.

### Verification

**Verdict:** potential

## Verification Record (Lightweight)

### Verdict: Potential

All claimed code locations verified in source. The complete code path from RPC submission through pool insertion to ack logic confirms that no deduplication of recipients exists, and `find_map` semantics prevent duplicate entries from ever being acknowledged.

### Neutral Analysis Summary

- **File confirmed**: `substrate/client/hop/src/pool.rs` exists at the specified commit.
- **Lines 234-236**: Confirmed — `recipients.is_empty()` check with no subsequent deduplication.
- **Lines 364-371**: `find_map` on `meta.recipients` always returns the first matching index. Finding originally cited lines 362-369 (slightly off).
- **Line 436**: `meta.claimed.iter().all(|&c| c)` confirmed exactly — requires all entries true before auto-deletion.
- **RPC layer** (`rpc.rs` lines 151-157, 193): SCALE-decodes recipients but performs no deduplication before passing to `pool.insert()`.
- **`types.rs` line 56**: `claimed = vec![false; recipients.len()]` creates one boolean per recipient including duplicates.
- **Full chain**: RPC passes undeduped recipients → insert stores them as-is → find_recipient always matches first occurrence → ack sets only first index → `all()` check never passes for duplicates → entry persists until expiry.

### Permalink Corrections

- Original (find_map): `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L362-L369`
- Corrected (find_map): `https://github.com/paritytech/polkadot-sdk/blob/6dceb293782d4944252d1d264bf99da378bbcfa2/substrate/client/hop/src/pool.rs#L364-L371`
- Other permalinks (lines 234-236, line 436): Unchanged — already correct.

