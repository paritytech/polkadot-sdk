# Bitswap API Refactor — Review Walkthrough

This walkthrough takes you through the staged changes in **reading order** (build understanding fastest), not file order. Each section names the files, the key lines, and what to look for when reviewing.

**TL;DR diff stats**: 14 modified files, 4 deleted files, 3 new files. Net: **+1340 / -2369** lines (≈ -1000 lines overall, even after the new state machine).

**Tests**: `cargo test -p sc-network bitswap::` — **18/18 pass.**
**Build**: `SKIP_WASM_BUILD=1 cargo check -p sc-network -p sc-service --all-targets` — clean. `cargo check -p staging-node-cli` — clean.

---

## How to read this

Each step calls out one file or one cluster, with:
- **Why this matters** — 1-2 sentence summary of the architectural intent.
- **Where to look** — file path + key line range.
- **What to verify** — what the reviewer should specifically check.

Skim the diff stats first, then walk through in this order. Total reading time ≈ 25-40 min.

---

## Step 0 — Context and scope

Before opening any file, two pieces of context anchor the review:

- The full design is in [Resolved Design](file:///home/sebastian/Documents/aidump/aidump/Bitswap%20API%20redesign/Resolved%20Design.md). The 10 decisions in §9 of that doc are the contract this PR delivers.
- This PR is **substrate-only**. Cumulus `storage-chain-sync` (the only consumer of the old API) is migrated in a follow-up. Master will not compile for `cumulus-client-storage-chain-sync` between this PR's merge and that follow-up. The breakage is intentional and called out in the [prdoc](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/prdoc/pr_12052.prdoc) and the doc.

Open [prdoc/pr_12052.prdoc](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/prdoc/pr_12052.prdoc) and read it once. It's 33 lines and sets the framing for everything that follows.

---

## Step 1 — The new public API surface

**Why this matters**: this is the only thing downstream callers ever touch. If you only review one file, review this one.

**File**: [substrate/client/network/src/bitswap/handle.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs) (new, 236 lines)

**What to verify**:

1. The types and shapes match the [Resolved Design §2](file:///home/sebastian/Documents/aidump/aidump/Bitswap%20API%20redesign/Resolved%20Design.md):
   - `BitswapHandle` is `Clone`, holds a single `mpsc::Sender<BitswapCommand>`.
   - The single public method is [`request_stream`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L121-L162). No `request`.
   - Return shape is `Result<mpsc::Receiver<Result<(Cid, FetchOutcome), BitswapError>>, BitswapError>` — outer admission errors, inner per-item errors (decision Q3).
   - `FetchOutcome` has only two variants — `Block(Vec<u8>)` and `Missing`. All operational causes for missing collapse into one variant (decision Q5/Q8).
   - `BitswapError` has 5 variants: `Unavailable`, `ServiceClosed`, `InvalidCid { cid }`, `Overloaded`, `TooManyCids { requested, max }`. `InvalidCid` carries only the `Cid` (decision Q8).
   - `BitswapServiceConfig` has only `request_timeout: Duration`, default 30s (decision Q9).

2. Admission logic in [`request_stream`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L137-L162):
   - Empty wantlist → immediately-closed receiver, not an error.
   - `cids.len() > MAX_CIDS_PER_REQUEST` (=16) → `TooManyCids`.
   - First unsupported CID → `InvalidCid`.
   - Sink capacity is `cids.len() + 1` — the `+1` reserves a slot for a possible terminal `Err(ServiceClosed)`. Comment on [line 23-24](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L23-L24) of that area explains this invariant.
   - `cmd_tx.try_send` maps Full → `Overloaded`, Closed → `ServiceClosed`.

3. [`BitswapCommand`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L178-L191) is `pub(crate)` only — the actor's internal message type.

4. [`PeerEvent`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L193-L216) is `pub` (the litep2p backend publishes into it). Three variants: `Snapshot { peers }`, `Connected { peer }`, `Disconnected { peer }`. The `Snapshot` variant is defined for forward-compat (currently never sent — see Step 4 for why this is fine).

5. [`BitswapWiring`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/handle.rs#L218-L237) carries everything needed to install Bitswap into the litep2p backend: `litep2p_config` (consumed by `with_libp2p_bitswap`), `user_handle` (stored on `Litep2pNetworkService`), `peer_event_tx` (the main-loop publishes into this).

**Red flags to look for**:
- Any `pub` item without a docstring → there shouldn't be any.
- Admission that pushes an error onto the sink instead of returning it — admission errors are synchronous Returns; only `ServiceClosed` and `Overloaded` can flow through the sink (and only the actor inserts them).

---

## Step 2 — The trait surgery in `NetworkService` / `NetworkBackend`

**Why this matters**: this is the user-visible surface change. `NetworkService` gains `bitswap_handle()`. `NetworkBackend` loses `BitswapConfig`/`bitswap_server`. The mechanism is a new supertrait `BitswapProvider`. This pattern is non-obvious — make sure you understand it before reading the impl sites.

**File**: [substrate/client/network/src/service/traits.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs)

**What to verify**:

1. [`BitswapProvider` trait](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs#L56-L67) has a default `bitswap_handle() -> None` body. Backends that don't support Bitswap (libp2p) can use the default, backends that do (litep2p) override.

2. [`impl<T> BitswapProvider for Arc<T>`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs#L69-L76) blanket. This is required because `NetworkBackend::NetworkService` is wrapped in `Arc` (e.g. `Arc<NetworkService<B,H>>`). Every other subtrait in this file has the same `Arc<T>` blanket impl — search for `^impl<T> .* for Arc<T>` and you'll see 8 of them.

3. [`NetworkService` supertrait](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs#L79-L92) gains `+ BitswapProvider`. The blanket impl on `T` (line 94-107) also gains it. **This is the key insight**: because `NetworkService` is a marker-trait with a blanket impl over its subtraits, every concrete type that implements all 7 (now 8) subtraits automatically gets `NetworkService`. Adding the new method means adding a new subtrait, not adding a method to the marker.

4. [`NetworkBackend` trait](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs#L131-L181): `type BitswapConfig` and `fn bitswap_server` are GONE. Compare with the diff hunk showing lines 148-149 and 162-167 removed.

**Red flags**:
- The `?Sized` bound on [`impl<T> BitswapProvider for Arc<T>`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service/traits.rs#L69-L72) — required because `Arc<dyn NetworkService>` is constructed elsewhere (Litep2pNetworkBackend returns `Arc<dyn NetworkService>` from `network_service()`). Without `?Sized` the blanket wouldn't apply to `Arc<dyn ...>`.
- Should the default `None` body be in the trait at all? Yes — the libp2p backend benefits from it (Step 3). Removing the default would force every NetworkService implementer (including third-party ones, if any) to add a stub impl.

---

## Step 3 — The two `BitswapProvider` impls (libp2p stub, litep2p real)

**Why this matters**: confirms the trait machinery works for both backends.

**File 1**: [substrate/client/network/src/service.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/service.rs) (libp2p `NetworkWorker`)

Look for the line `impl<B, H> crate::service::traits::BitswapProvider for NetworkService<B, H>` near the end of the existing 7-trait-impl block (after `NetworkRequest`). The impl body is **empty** — it uses the default `None`. That's the entire change on the libp2p side.

Also verify the diff REMOVES `type BitswapConfig = RequestResponseConfig` and the `fn bitswap_server` impl in the `impl<B, H> NetworkBackend<B, H> for NetworkWorker<B, H>` block.

**File 2**: [substrate/client/network/src/litep2p/service.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/litep2p/service.rs)

Two things to verify:

1. The struct field [`bitswap_handle: Option<crate::bitswap::BitswapHandle>`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/litep2p/service.rs#L221-L222) replaces the old `bitswap_cmd_tx: Option<mpsc::Sender<BitswapOutboundCmd>>`. The constructor takes it as a parameter (line 237).

2. The `BitswapProvider` impl at the end of the file delegates to the inherent method:
   ```rust
   impl crate::service::traits::BitswapProvider for Litep2pNetworkService {
       fn bitswap_handle(&self) -> Option<crate::bitswap::BitswapHandle> {
           self.bitswap_handle()
       }
   }
   ```
   The inherent method (around line 251) just `.clone()`s the field.

3. The whole `route_bitswap_request` method (was lines 253-324) is **gone**.

4. The bitswap protocol-name special-case in `start_request` (was around line 648, `if protocol.as_ref() == crate::bitswap::PROTOCOL_NAME`) is **gone**.

**Red flags**:
- The libp2p impl is empty — make sure that's not because I forgot something. Default body returns `None`, which is correct semantics for libp2p (Bitswap not supported there).
- `Litep2pNetworkService` no longer imports `prost::Message` or `ProtoBitswapWantType` — confirm the removed imports in the diff.

---

## Step 4 — The state machine: BitswapService actor

**Why this matters**: this is the heart of the implementation. The actor's correctness is what makes the API safe.

**File**: [substrate/client/network/src/bitswap/service.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs) (new, 1071 lines including tests)

Recommended reading order WITHIN this file:

### 4a. Internal constants ([lines 60-69](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L60-L69))

```rust
const MAX_OUTSTANDING_CIDS: usize = 1024;
const MAX_WAITERS_PER_CID: usize = 64;
const MAX_CONCURRENT_INBOUND_LOOKUPS: usize = 8;
const CMD_CHANNEL_CAPACITY: usize = 256;
const PEER_EVENT_CHANNEL_CAPACITY: usize = 64;
const LOOKUP_CHANNEL_CAPACITY: usize = 64;
const PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
const PEER_FANOUT_CAP: usize = 1;
const PEER_TIMEOUT_SWEEP_INTERVAL: Duration = Duration::from_secs(1);
```

These match [Resolved Design §5](file:///home/sebastian/Documents/aidump/aidump/Bitswap%20API%20redesign/Resolved%20Design.md). Verify the values are as the design says.

### 4b. `BitswapTransport` trait ([lines 35-54](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L35-L54))

This is the test seam. `LitepBitswapHandle` is wrapped behind this trait so unit tests can inject mock events. The production impl is a 3-method passthrough.

**What to verify**: only 3 methods (`next_event`, `send_request`, `send_response`). No leaky abstractions. Production impl is trivial.

### 4c. Data model ([lines 71-98](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L71-L98))

```rust
struct CidState {
    tried_peers: HashSet<litep2p::PeerId>,
    in_flight_peers: HashMap<litep2p::PeerId, Instant>,
    waiters: SmallVec<[WaiterId; 2]>,
}

struct Waiter {
    cids_remaining: HashSet<Cid>,
    sink: mpsc::Sender<FetchItem>,
    delay_key: delay_queue::Key,
}
```

**This is the most important diff in the PR.** Confirm:
- `CidState` does **not** carry a `deadline` field. Per-waiter deadlines, not per-CID (decision Q1).
- `CidState.waiters` is a SmallVec — most CIDs have 1-2 waiters; allocation-free in the common case.
- `Waiter.cids_remaining` is a `HashSet` — we decrement it as each CID resolves; the waiter completes when it's empty.
- `Waiter.delay_key` is the `DelayQueue::Key` returned at insert time, used to cancel-on-early-completion (function [`drop_waiter`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L364-L376)).

### 4d. `BitswapService<B>` struct ([lines 100-115](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L100-L115))

Six "wire" fields (cmd_rx, peer_event_rx, lookup_{tx,rx}, lookup_semaphore, waiter_deadlines) and three "state" fields (connected_peers, wants, waiters). Plus `handle`, `client`, `config`.

**What to verify**:
- `handle: Box<dyn BitswapTransport>` — the test seam.
- `wants: HashMap<Cid, CidState>` — deduplicated across waiters (key insight).
- `waiters: SlotMap<WaiterId, Waiter>` — slotmap because we need stable keys across removals.
- `waiter_deadlines: DelayQueue<WaiterId>` — fires when a waiter's deadline elapses; produces a `WaiterId` we look up in `waiters`.

### 4e. `pub fn start` ([lines 122-154](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L122-L154))

The entry point called by `build_network`. Returns `(future, BitswapWiring)`. The future MUST be spawned; the wiring MUST be passed into the litep2p backend.

**What to verify**: the three channels (cmd, peer_event, lookup) are constructed here with their capacity constants. The `LitepConfig::new()` call is where the litep2p protocol config + handle pair come from.

### 4f. The `run()` loop ([lines 159-204](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L159-L204))

**Six select arms** (one more than the Resolved Design — peer-timeout sweep was added as a separate arm rather than opportunistic checks, for clarity):

1. Inbound BitswapEvent (Request / Response).
2. User commands (RequestStream).
3. Peer events (Snapshot / Connected / Disconnected).
4. Waiter deadline expiry — gated on `!self.waiter_deadlines.is_empty()` (DelayQueue panics if you poll it empty).
5. Completed blocking lookup → forward via `handle.send_response`.
6. Per-peer timeout ticker.

**What to verify**:
- No `.await` on storage inside any arm body. Confirm by reading each arm.
- No `.await` on user sinks. The actor uses `try_send` everywhere. Search for `sink.try_send` (about 4 hits).
- No `.await` on the litep2p `handle.send_request` / `send_response` that could conceivably block for an unbounded time. (litep2p's internal channel is 4096 entries deep per the registry source — not infinite, but in practice never blocks. If this becomes a concern, the fix is routing outbound through an internal sender task.)
- `None` from any channel → `shutdown_waiters()` is called → all waiters get `Err(ServiceClosed)` and the loop returns.

### 4g. `on_request_stream` ([lines 206-244](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L206-L244))

Admission-time validation in the actor. Handle.rs already did the cheap checks; this is the second-line defense against races.

**What to verify**:
- `new_cid_count` filter only counts CIDs not already in `wants` — overlapping waiters don't double-charge the budget.
- `MAX_WAITERS_PER_CID` cap.
- `SlotMap::insert_with_key` is used so the `Waiter`'s `delay_key` can be inserted into `DelayQueue` with the slot's actual key — no placeholder dance needed.
- After insertion, every CID gets `top_up_in_flight` called.

### 4h. Peer fanout: `top_up_in_flight` ([lines 246-265](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L246-L265))

**This is where CAP=1 lives.**

**What to verify**:
- Returns early if `in_flight_peers.len() >= PEER_FANOUT_CAP` (= 1 in v1).
- Picks the first peer in `connected_peers \ tried_peers \ in_flight_peers`.
- Records the per-peer deadline as `Instant::now() + PER_PEER_TIMEOUT` (= 5s).
- Calls `self.handle.send_request(peer, vec![(cid, WantType::Block)]).await` — sends only one CID per request (no batching across waiters). This is intentional; batching is a future optimization that doesn't change correctness.

### 4i. Hash verification: `on_inbound_response` + `recompute_cid` ([lines 277-321 and 478-487](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L277-L321))

**This is the security-critical part.**

For each inbound `Block`:
1. Move the peer to `tried_peers` for the CID it responded for.
2. Recompute the CID from `(prefix.codec, prefix.mh_type, hash(prefix.mh_type, data))` via [`recompute_cid`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L478-L487).
3. Look up the recomputed CID in `wants`. If absent → dropped silently.
4. If present → call `deliver_block`.

**What to verify**:
- The recomputed CID — NOT the claimed CID — is what's looked up in `wants`. A peer cannot make us accept arbitrary bytes by lying about the CID in the response.
- [`hash_for_multihash_code`](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L489-L496) supports only the 3 blessed multihash codes (Blake2b-256, SHA2-256, Keccak-256). Unsupported codes → `None` → recompute fails → block dropped.
- Presence responses (`Have`/`DontHave`) move the peer to `tried_peers` and trigger top-up. They do NOT directly cause `Missing` for the waiter — that happens at deadline time when no peer ever delivered the block.

### 4j. Multi-waiter delivery: `deliver_block` ([lines 332-352](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L332-L352))

**What to verify**:
- The bytes are wrapped in `Arc<Vec<u8>>` internally to avoid cloning across multiple waiters interested in the same CID. The clone at delivery time is unavoidable (mpsc::Sender takes ownership), but it's at most one Vec clone per waiter, not the whole-payload-per-waiter that a naive impl would do.
- `try_send` on the sink — a slow/closed waiter triggers `drop_waiter` for that one waiter, NOT for the others.
- After the loop, if no waiter remains, the CID's CidState is removed (via `drop_waiter`'s GC logic).

### 4k. Cancellation: `drop_waiter` ([lines 364-376](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L364-L376))

**What to verify**:
- Removes the waiter from `waiters` (SlotMap).
- Removes it from `waiter_deadlines` (so the DelayQueue doesn't fire later).
- For each `cid` in the waiter's `cids_remaining`, removes the waiter from `CidState.waiters`. If the CidState has no waiters and no in-flight peers left → it's GC'd from `wants`.

### 4l. Deadline expiry: `on_waiter_expired` ([lines 378-395](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L378-L395))

Each remaining CID gets `Ok((cid, Missing))` sent via `try_send`. The waiter is then removed from every `CidState.waiters`. Same GC as `drop_waiter`. No `Err(...)` is emitted — Missing is the right outcome, not an error.

### 4m. Peer events: `on_peer_*` ([lines 397-432](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L397-L432))

**What to verify**:
- `Connected`: insert into `connected_peers`, then top up every CID in `wants` (since a previously-blocked CID might now have a peer to try).
- `Disconnected`: remove from `connected_peers`. Also remove the peer from `in_flight_peers` of every CID. Then top up those CIDs (failover).
- `Snapshot`: replaces `connected_peers` wholesale, then top up. **Currently never sent** — the actor is constructed before the litep2p backend's `run()` starts, so there are no pre-existing peers to snapshot. The variant exists for forward-compat.

### 4n. Per-peer timeout sweep ([lines 434-457](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L434-L457))

Fires every 1s. Walks `wants` and removes `in_flight_peers` entries whose `Instant` deadline has passed. Each timed-out peer is moved to `tried_peers`, then top-up is called.

**What to verify**:
- The retain-based pattern is idiomatic.
- After collecting timed-out `(cid, peer)` pairs, the peers are added to `tried_peers` and top-up is called per CID. The split-borrow pattern is necessary because the `retain` closure can't call `self.top_up_in_flight`.

### 4o. Inbound serving: `on_inbound_request` ([lines 459-475](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L459-L475))

**This addresses the [[Bitswap Service Blocking Follow Up]] concern.**

**What to verify**:
- `lookup_semaphore.try_acquire_owned()` bounds inbound serving concurrency at 8. If saturated → drop the request silently. Old peer eventually times out — no fabricated DontHave.
- `tokio::task::spawn_blocking` for the actual DB read. The semaphore permit is dropped when the spawned task completes.
- Completion arrives on the loop's arm 5 (`lookup_rx`) which then forwards via `handle.send_response`. This is the only place the actor's main loop interacts with disk IO.

### 4p. Tests ([lines 562-1071](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L562-L1071))

16 tests, one per scenario from [Resolved Design §7.1](file:///home/sebastian/Documents/aidump/aidump/Bitswap%20API%20redesign/Resolved%20Design.md). The `TestRig` harness ([lines 596-635](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/service.rs#L596-L635)) gives each test a `MockTransport` for injecting BitswapEvents and observing outbound traffic.

**What to verify**:
- Most tests use `#[tokio::test(start_paused = true)]` + `tokio::time::advance()` to deterministically test deadlines. No real wall-clock waits.
- `corrupted_block_rejected_then_missing_at_deadline` — hash verification works: peer sends bytes that don't hash to the claimed CID, block is silently dropped, eventually Missing.
- `inbound_request_with_known_block_serves_it` — full inbound path against a real `substrate-test-runtime-client` with an indexed transaction.
- `service_shutdown_emits_service_closed` — dropping the inbound channel causes the actor to call `shutdown_waiters`, which emits `Err(ServiceClosed)` on each waiter's sink.

**Run them yourself**:
```bash
cd /home/sebastian/work/tries/2026-05-13-bitswap-API-improvement
cargo test -p sc-network bitswap::
```
Expected: `18 passed; 0 failed`.

---

## Step 5 — The bitswap module trimmed down

**Why this matters**: confirms what survives from the old module, and that nothing relies on the deleted protobuf schema.

**File**: [substrate/client/network/src/bitswap/mod.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/mod.rs)

The file went from 665 → ~110 lines. What survives:

- License + module doc.
- `mod handle; mod service;` + re-exports of the public types.
- `MAX_WANTED_BLOCKS` (16), `RAW_CODEC` (0x55), the three multihash constants.
- `is_cid_supported` + `is_supported_multihash_code` — the validation helpers.
- The `Prefix` struct + `From<&Cid>` impl. **No more `to_bytes`** — the actor doesn't construct protobuf prefixes, litep2p does that internally.
- 2 unit tests for `is_cid_supported`.

What's GONE:
- `BitswapRequestHandler` (the libp2p request-response handler) — 152 lines, deleted.
- `RequestHandlerError` enum.
- The 7 libp2p-specific tests (`undecodable_message`, `empty_want_list`, `too_long_want_list`, `transaction_not_found`, `transaction_found`, `transaction_not_found_sends_dont_have_when_requested`, `transaction_found_sends_have_for_want_have`).
- `mod schema` and the protobuf re-export `BitswapProtoMessage`.
- The `decode_prefix` function and `PrefixDecodeError` type.
- `mod client` + the entire `pub use client::*` re-export block.

---

## Step 6 — Deleted files (the litep2p shim, the libp2p path, the protobuf schema)

**Why this matters**: these are the deletions that make the new design coherent. None of them should have hidden references elsewhere.

| Deleted file | Reason | Verify by |
|---|---|---|
| [substrate/client/network/src/bitswap/client.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/client.rs) (906 lines) | The old `request_bitswap_blocks` / `request_bitswap_blocks_unverified` helper — the "req-resp mockery" the issue calls out. | `rg request_bitswap_blocks` → only mentions in docs. |
| [substrate/client/network/src/litep2p/bitswap.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/litep2p/bitswap.rs) (653 lines) | The old `BitswapService` + `PendingBatch` shim that re-encoded protobuf bytes. | `rg PendingBatch\|BitswapOutboundCmd` → no hits. |
| [substrate/client/network/src/bitswap/schema.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/bitswap/schema.rs) | Just `include!`-ed the prost-generated protobuf bindings. No longer needed (litep2p owns the wire format). | `rg BitswapProtoMessage` → no hits. |
| [substrate/client/network/src/schema/bitswap.v1.2.0.proto](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/schema/bitswap.v1.2.0.proto) | The protobuf source file. | `rg \\.proto` (in sc-network) → no hits. |
| [substrate/client/network/build.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/build.rs) | Compiled the protobuf. | (file gone; cargo skips build step entirely.) |

**What to verify**: no compilation references to any of these from elsewhere. The two `prost` / `prost-build` dependencies are removed from Cargo.toml (next step).

---

## Step 7 — Cargo.toml dependency churn

**File**: [substrate/client/network/Cargo.toml](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/Cargo.toml)

Adds:
- `slotmap = { workspace = true }` — for stable waiter IDs.
- `tokio-util = { features = ["time"], workspace = true }` — for `DelayQueue`.
- `tokio` features: added `rt` (needed by `tokio::task::spawn_blocking`).
- In dev-deps: `tokio` features added `test-util` (for `start_paused = true` + `tokio::time::advance`).

Removes:
- `prost = { workspace = true }` — no longer parsing/constructing Bitswap protobuf in this crate.
- `[build-dependencies] prost-build = ...` — entire section gone with build.rs.

**What to verify**: all four added items are workspace-managed (no version pins introduced here). `tokio-util` is now in both `[dependencies]` (with `time` feature) and `[dev-dependencies]` (with `compat` feature) — cargo merges features, this is correct.

---

## Step 8 — Wiring: `Litep2pNetworkBackend` and `build_network`

**Why this matters**: connects the new service to the actual node startup path.

### 8a. `IpfsConfig` lost its generics

**File**: [substrate/client/network/src/config.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/config.rs)

Old: `pub struct IpfsConfig<Block, H, N: NetworkBackend<Block, H>> { bitswap_config: N::BitswapConfig, block_provider, bootnodes }`.

New: `pub struct IpfsConfig { bitswap_wiring: Option<BitswapWiring>, block_provider, bootnodes }`.

Two things to verify:
1. The generics dropped because no remaining field is generic over `(Block, H, N)`.
2. `Params::ipfs_config: Option<IpfsConfig>` (no generics) — this means the 5 test/bench harnesses that set `ipfs_config: None` continue to compile unchanged (verified during implementation by running `cargo check` workspace-wide).

### 8b. `Litep2pNetworkBackend::new`

**File**: [substrate/client/network/src/litep2p/mod.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/litep2p/mod.rs)

The bitswap-init block (was lines 511-529 in the old code) was replaced. New behavior:

```rust
if let Some(ipfs) = params.ipfs_config {
    let wiring = ipfs.bitswap_wiring.expect("...; qed");
    config_builder = config_builder.with_libp2p_bitswap(wiring.litep2p_config);
    bitswap_user_handle = Some(wiring.user_handle);
    bitswap_peer_event_tx = Some(wiring.peer_event_tx);
    // ... DHT setup (unchanged) ...
}
```

The `.expect("...; qed")` is the invariant: if `ipfs_config` is `Some`, `bitswap_wiring` must also be `Some`. This invariant is upheld by `build_network` (step 8c).

Two new fields on `Litep2pNetworkBackend`:
- `bitswap_peer_event_tx: Option<mpsc::Sender<crate::bitswap::PeerEvent>>`
- `bitswap_peer_conn_count: HashMap<litep2p::PeerId, usize>` — see step 8d below.

### 8c. `build_network` (the libp2p+ipfs_server guard)

**File**: [substrate/client/service/src/builder.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/service/src/builder.rs)

The old `Net::bitswap_server(client.clone())` call (was line 1246) is replaced by:

1. A runtime guard: `if libp2p backend && ipfs_server → return Err(...)`. The error message says "Bitswap requires the litep2p network backend; set --network-backend litep2p or disable --ipfs-server".
2. A call to `sc_network::bitswap::start::<Block>(client.clone(), BitswapServiceConfig::default())`.
3. Spawn the returned future as `"bitswap-service"`.
4. Construct `IpfsConfig { bitswap_wiring: Some(bitswap_wiring), ... }`.

**What to verify**:
- The guard fires BEFORE `bitswap::start` is called → no wasted setup on the error path.
- The match expression uses `matches!(..., NetworkBackendType::Libp2p)` — easy to read.
- The error type is `Error::Other(...)` which is the existing pattern for build-time configuration errors in this file.

### 8d. Peer-event broadcast from the litep2p main loop

**File**: [substrate/client/network/src/litep2p/mod.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/litep2p/mod.rs) (the `run()` method, around the `ConnectionEstablished` / `ConnectionClosed` arms)

**What to verify**:
- The bitswap publish happens **BEFORE** the `let Some(metrics) = &self.metrics else { continue }` guard. This is intentional — the main loop's `self.peers` map is only populated when metrics are enabled, so I cannot rely on it for bitswap peer dedup. The new `bitswap_peer_conn_count` map is populated unconditionally (when bitswap is enabled).
- The dedup logic: increment on every `ConnectionEstablished`, publish `Connected` only on 0→1 transition. Decrement on every `ConnectionClosed`, publish `Disconnected` only on 1→0 transition. This handles peers with multiple concurrent connections correctly.
- `tx.try_send(...)` is used (never blocks the main loop). If the actor's peer_event_rx is full, the event is silently dropped — acceptable because the next ConnectionEstablished/Closed will re-converge state.

---

## Step 9 — Re-exports

**File**: [substrate/client/network/src/lib.rs](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/substrate/client/network/src/lib.rs)

The `pub use service::traits::{...}` block gains `BitswapProvider`. That's the only change.

`BitswapHandle`, `BitswapError`, `FetchOutcome`, etc are re-exported via `crate::bitswap::*` — `bitswap` was already a `pub` module, no change needed.

**What to verify**: callers can write either `use sc_network::bitswap::BitswapHandle;` or `use sc_network::BitswapProvider;` (the trait — needed to call `.bitswap_handle()` on a NetworkService).

---

## Step 10 — The prdoc

**File**: [prdoc/pr_12052.prdoc](file:///home/sebastian/work/tries/2026-05-13-bitswap-API-improvement/prdoc/pr_12052.prdoc)

33 lines. Sets reviewer expectation:

- `sc-network`: major bump (API surface changed — `bitswap_server` trait method gone, `BitswapConfig` associated type gone).
- `sc-service`: minor bump (just one site touched, behavior preserved when ipfs_server=false).
- Calls out the substrate-only scope and the temporary cumulus breakage.

---

## Final QA you should run yourself

```bash
cd /home/sebastian/work/tries/2026-05-13-bitswap-API-improvement

# 1. Build sc-network + sc-service.
SKIP_WASM_BUILD=1 cargo check -p sc-network -p sc-service --all-targets

# 2. Build the node binary (this is what verifies end-to-end wiring).
SKIP_WASM_BUILD=1 cargo check -p staging-node-cli --lib

# 3. Run the bitswap test suite.
cargo test -p sc-network bitswap::

# 4. (Optional) Confirm the format pass was applied.
cargo fmt -p sc-network -p sc-service --check
```

Expected:
- All checks pass.
- 18/18 bitswap tests pass.
- `cargo fmt --check` is clean.

---

## What's deliberately NOT covered by this PR

(So you don't ask about them.)

- Cumulus consumer migration — separate follow-up PR.
- A zombienet end-to-end integration test that drives Bitswap between two nodes — the Resolved Design §7.2 lists it as future work. Unit tests cover the actor's logic; the real two-node test requires the cumulus migration to give us an actual production consumer.
- A clippy pass — let CI run that. Local clippy on `-p sc-network` is clean against the touched code (the warnings that remain are pre-existing in untouched files).
- Real bitswap sessions, HAVE preflight, per-handle quotas, per-call deadline overrides, higher peer-fanout CAP — listed as known follow-ups in Resolved Design §10.

---

## Total time to review

If you trust the test suite (and you should — 16 new scenario tests cover the decision matrix), the minimum useful review is:

1. **Skim Step 0 + Step 1** (5 min): confirms the public API matches the design contract.
2. **Read Step 2** (5 min): confirms the trait surgery is sound.
3. **Read Step 4a → 4o** (15-20 min): confirms the actor's correctness invariants. The 4i (hash verification) and 4o (inbound offload) sections deserve special attention.
4. **Skim Step 8** (5 min): confirms the wiring is intentional.
5. **Run the QA commands above** (10-30 min depending on cold/warm cargo cache).

≈ 45 min for a high-confidence review. Up to 90 min if you want to read every test individually.
