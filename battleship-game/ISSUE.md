# Battleship Game — Open Issues

## 1. PAPI `bestBlocks$` stops emitting after initial burst

### Observed behavior

After connecting via smoldot, `client.bestBlocks$` emits a few initial events then stops entirely. The subscription never fires again even though smoldot continues to receive and verify new parachain blocks.

### Evidence

```
bestBlocks$=1873678 query(best)=1873775 behind=-97
bestBlocks$=1873678 query(best)=1873779 behind=-101
bestBlocks$=1873678 query(best)=1873783 behind=-105
bestBlocks$=1873678 query(best)=1873798 behind=-120
```

`bestBlocks$` stays at 1873678 while `query(best)` advances to 1873798 and beyond. The `chainHead_v1_follow` subscription appears to stop delivering `bestBlockChanged` events to PAPI's observable.

### Reproduction

```typescript
const client = createClient(getSmProvider(() => smoldot.addChain({...})));
await api.query.System.Number.getValue({ at: "best" });

let bestBlocksNumber = 0;
client.bestBlocks$.subscribe({
  next: (blocks) => { bestBlocksNumber = blocks[blocks.length - 1].number; }
});

setInterval(async () => {
  const queryNumber = await api.query.System.Number.getValue({ at: "best" });
  console.log(`bestBlocks$=${bestBlocksNumber} query(best)=${queryNumber}`);
}, 1000);
```

After ~10-30 seconds, `bestBlocks$` stops updating while `query(best)` continues to return newer block numbers.

### Impact

The bot's game loop was driven by `bestBlocks$`. When it stops, the bot stops processing game actions entirely. Workaround: use a 200ms polling timer with `getGame({ at: "best" })` instead of `bestBlocks$`.

### Additional observation

Even with `getGame({ at: "best" })` polling, the bot sometimes returns stale data for extended periods (2+ minutes). During the same period, `System.Number` queries at `"best"` return current values. This suggests the staleness may be specific to certain storage keys or that the `chainHead` subscription intermittently stalls.

### Environment

- smoldot: custom branch `bkchr-battleship` (includes elastic scaling PR fix-2158)
- polkadot-api: `^2.0.0-canary.f7f4b6e` (UI) / `^2.0.0-rc.2` (bot)
- Parachain: passet-hub-elastic on Paseo, 3 collators, slot_duration=6000ms, elastic scaling producing ~2 blocks/sec

---

## 2. Collator `RemoteCouldntAnswer` on light client call-proof requests

### Observed behavior

Smoldot requests call proofs from collators for `TaggedTransactionQueue_validate_transaction`. The collators respond with an empty proof (`proof: None`), causing smoldot to log `RemoteCouldntAnswer` and ban the peer for 10 seconds.

### Root cause investigation

The Substrate light client request handler (`substrate/client/network/light/src/light_client_requests/handler.rs`) calls `self.client.execution_proof(block, method, data)`. When this fails, it returns `RemoteCallResponse { proof: None }`. The original error was logged at `trace!` level, making it invisible.

**Fix applied**: Changed the logging to `warn!` level in the handler for both `on_remote_call_request` and `on_remote_read_request` failures. The new collator binary with this logging has been deployed to all 3 collators.

### Current status

With the new binary deployed, **no FAILED entries appeared in the collator logs** during the latest test runs. The call-proof failures stopped. This needs further monitoring — the failures may have been caused by the old binary's state management under heavy block production.

### Collator deployment

All 3 collators now run the custom binary built from this repo with the enhanced logging:

- `195.154.218.130` (Alice) — `/home/ubuntu/polkadot-omni-node` with libs in `/home/ubuntu/collator-libs/`
- `195.154.212.193` (Bob) — same setup
- `195.154.91.21` (Charlie) — same setup

Start command includes `-l light-client-request-handler=debug`. Requires `LD_LIBRARY_PATH=/home/ubuntu/collator-libs` since the binary was built on NixOS.

---

## 3. Statement protocol substream reliability

### Observed behavior

The statement protocol substream between smoldot light clients and collator full nodes is flaky. Sometimes the collator opens the statement substream to smoldot successfully, sometimes it doesn't. When it fails, statement propagation doesn't work and the lobby discovery fails.

### Fix applied in smoldot

`smoldot/lib/src/network/service.rs`: When an inbound Statement `NotificationsInOpen` arrives from a full node, smoldot now eagerly accepts it and records the substream as Open. Previously, it would go to Pending state and potentially time out before `gossip_open` was called.

Smoldot no longer opens outbound Statement substreams — it only accepts inbound ones initiated by full nodes. The full nodes add smoldot peers to the statement protocol's reserved set via `SyncEvent::PeerConnected`.

### Remaining flakiness

The statement propagation still fails in ~30-40% of test runs. The collator's `PeerConnected` → `AddReservedPeers` → open substream flow sometimes doesn't complete before the test's lobby discovery timeout (300s). This needs further investigation into the litep2p peerset timing.

---

## 4. Bot merkle proof — `InvalidMerkleProof` (likely resolved)

### Observed behavior

The bot's cell reveal was rejected on-chain with `InvalidMerkleProof`. This happened intermittently.

### Investigation

The merkle proof generation code is identical between bot and UI. The tree construction, leaf encoding (33 bytes: salt + is_occupied), and proof generation all match the on-chain `binary_merkle_tree::verify_proof` expectations.

Debug logging confirmed `rootMatch=true` (rebuilt tree root matches committed root) and `proofLen=7` (correct for 100 leaves).

### Likely root cause

The `InvalidMerkleProof` errors correlated with the bot spamming duplicate attacks due to missing dedup guards. Multiple attacks at the same round with different nonces caused race conditions where the on-chain state advanced unexpectedly.

### Fixes applied

- Added `lastAttackRound` and `lastAttackTarget` to prevent duplicate attacks and ensure retries use the same target cell
- Added `lastRevealRound` and `lastRevealTime` to prevent duplicate reveals
- Cached merkle proofs at game start (`state.myProofs`) instead of rebuilding the tree on every reveal

### Status

No `InvalidMerkleProof` errors observed in recent test runs after the dedup fixes. Needs confirmation over 10+ consecutive games.

---

## 5. Browser smoldot bootstrap flakiness

### Observed behavior

The browser's smoldot sometimes takes 2-5+ minutes to deliver the first best block via PAPI, and occasionally never delivers it within the test timeout.

### Details

The browser connects to all 3 collators via `wss://ip:443`, downloads the runtime, and starts receiving block announces. The `chainHead_v1_follow` subscription's `initialized` event fires, followed by `bestBlockChanged`. But sometimes this flow stalls — the browser stays at `Waiting for first best block...` indefinitely.

The bot (Node.js smoldot) connecting to the same collators via `ws://ip:30333` bootstraps faster and more reliably.

### Possible causes

- TLS connection overhead for `wss://ip:443` adds latency
- Browser WebSocket scheduling differs from Node.js
- Chromium headless mode may throttle web workers running smoldot

---

## Summary of smoldot changes

All changes are in the local smoldot repo at `/home/bastian/projects/parity/smoldot` on branch `bkchr-battleship`:

1. `lib/src/network/service.rs` — Statement substream: accept inbound eagerly, don't open outbound
2. `light-base/src/platform/address_parse.rs` — Parse `/ip4/.../tcp/port/wss` and `/ip6/.../tcp/port/wss` multiaddrs for secure WebSocket over IP
3. `light-base/src/platform.rs` — Added `secure: bool` field to `WebSocketIpv4` / `WebSocketIpv6` connection types and `WebSocketIp` address
4. `light-base/src/platform/default.rs` — Handle secure vs non-secure WebSocket IP connections
5. `wasm-node/rust/src/platform.rs` — Connection type tags 12/13 for secure WebSocket IPv4/IPv6
6. `wasm-node/rust/src/bindings.rs` — Updated connection type documentation
7. `wasm-node/javascript/src/internals/local-instance.ts` — JS-side handling of tags 12/13 for `wss://ip:port`
8. `light-base/src/runtime_service.rs` — Changed `foreground-runtime-call-progress-fail` log from Trace to Warn

## Summary of polkadot-sdk changes

1. `substrate/client/network/light/src/light_client_requests/handler.rs` — Changed trace logs to warn/debug for light client request handling (success and failure)
