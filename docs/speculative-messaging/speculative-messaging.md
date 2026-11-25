# Speculative Messaging on JAM: Inclusion Tier

Parachains on JAM's Parachain Service exchange messages offchain rather than routing payloads
through the relay or Parachain Service.
Receiving collators fetch messages directly from the sender's nodes, include them in a block, and
prove them against a cryptographic
root the sender previously committed to JAM.

The primitives and encoding rules described here are implemented in
`cumulus/primitives/spec-messaging`.

## 1. Architecture and Terminology

Throughout the document, A is the sending parachain and B is the receiving parachain.

| Term | Meaning |
|---|---|
| Stream | An append-only Merkle Mountain Range (MMR) of payloads, identified by a `StreamId`. |
| Register | B's latest `Register` leaf on an acknowledgment stream. It tells A how far B has consumed and the current channel status. |
| Candidate | A single Parachain Service work item. Extrinsic 0 is its PoV, which holds exactly one block and, outside that block, its lifts. |
| `TargetRoot` | A `StreamsRoot` selected from A's settlement ring at the anchor block, used to verify incoming messages for a candidate.  |
| Cursor | The next leaf position a chain will read from an inbound stream. |
| Lift | Work-item data (MMR extension and Patricia trie proof) proving that the stream state consumed by B is a valid prefix of A's stream under `TargetRoot`. |


### End-to-End Flow

```
┌─ Sender A ────────────────┐     ┌─ Parachain Service ───────┐     ┌─ Receiver B ──────────────┐
│                           │     │                           │     │                           │
│ Appends payloads to       │────▶│ Accumulates A's candidate │────▶│ Collator fetches messages │
│ stream MMRs               │     │ and pushes StreamsRoot    │     │ from A's archive nodes    │
│                           │     │ into A's settlement ring  │     │                           │
│ Emits StreamsRoot in      │     │                           │     │ Picks TargetRoot from A's │
│ SPMS header digest        │     │                           │     │ ring and verifies fetches │
│                           │     │                           │     │                           │
│ Refine declares root      │     │                           │     │ Builds block with         │
│ (host call 204)           │     │                           │     │ verified inherent data    │
└───────────────────────────┘     ├───────────────────────────┤     ├───────────────────────────┤
                                  │                           │     │                           │
                                  │ Accumulates B's candidate │◀────│ Refine verifies lifts     │
                                  │ only if every declared    │     │ under TargetRoot and      │
                                  │ root exists in sender's   │     │ declares (A, TargetRoot)  │
                                  │ settlement ring           │     │ (host call 205)           │
                                  └───────────────────────────┘     └───────────────────────────┘
```

## 2. Streams and Proofs

### Hashing

All hashes use **BLAKE2b-256** prefixed with a single-byte domain tag so hashes from one context
cannot collide with another.
Unless noted otherwise, integers use SCALE little-endian encoding.

| Tag | Name | Preimage | Notes |
|---|---|---|---|
| `0x01` | Message leaf | `0x01 ‖ 0x00 ‖ payload` | `0x00` is the leaf version. Source, stream, and position are bound by tree position rather than the preimage. |
| `0x02` | MMR inner node | `0x02 ‖ left ‖ right` | Merges two MMR child hashes. |
| `0x03` | Bagging step | `0x03 ‖ acc ‖ peak` | Folds MMR peaks into a single root. |
| `0x04` | `MmrRoot` | `0x04 ‖ leaf_count (u64) ‖ bag(peaks)` | Includes `leaf_count` because peak hashes do not encode height and could otherwise collide across different counts. |
| `0x05` | Trie leaf | `0x05 ‖ stream_key ‖ MmrRoot` | Binds a stream's `MmrRoot` to its 8-byte key. |
| `0x06` | Trie inner node | `0x06 ‖ split_bit (u8) ‖ left ‖ right` | Binds the split bit so siblings cannot be shifted to a different trie depth. |

Peak bagging `bag(peaks)`
- No peaks: 32 zero bytes (`[0u8; 32]`).
- One peak: The peak hash itself.
- Multiple peaks: Initialize `acc` to the lowest peak, then iterate upward through each higher peak
  `P` via `acc = H(0x03 ‖ acc ‖ P)`. Note that `acc` (the right side) comes first. This reverses the
  usual `left ‖ right` order on purpose to match `bagging_peaks_hashes` in `mmr_lib` v0.8.3.

### Stream Identifiers

```rust
enum StreamId {
    Channel { recipient: ParaId, domain: u8, num: u16 },
    Ack { recipient: ParaId, domain: u8, num: u16 },
    // All other variants are reserved and rejected
}
```

Trie keys use the compact 8-byte form (`to_bytes()`) with integers encoded in big-endian. SCALE
encoding is used only on the wire.

A channel from A to B has two streams:
  1. A's `Channel { recipient: B, domain, num }` (carries messages from A to B).
  2. B's `Ack { recipient: A, domain, num }` (carries B's `Register` updates back to A).


Chains agree on `(domain, num)` outside the protocol, usually through governance. A chain cannot
open a channel to itself.

### Frontiers, StreamsRoot, and Extension Proofs

- `MmrFrontier { leaf_count, peaks }` is the compact MMR state ordered largest peak first. A
  frontier from an untrusted source is only valid if `leaf_count <= 2^48` and
  `peaks.len() == popcount(leaf_count)`.
- `StreamsRoot` is a binary Patricia trie mapping every active 8-byte stream key to its `MmrRoot`.
  This includes every stream the chain has ever written to.
- `StreamProof` is a path from a leaf up to the root made of `(split_bit, sibling)` pairs. Verifiers
  must reject any proof where the `split_bit` values are not strictly decreasing.
- A block that writes to at least one stream puts exactly one
  `DigestItem::Consensus(*b"SPMS", StreamsRoot)` in its header digest. Blocks with no stream writes
  add nothing.
- `MmrExtensionProof { leaf_count, connecting_nodes }` moves a stream frontier from `n1` leaves to
  its `MmrRoot` at `n2 = leaf_count` leaves.
  - Contains at most 48 nodes and no explicit positions, since positions are worked out from
    `(n1, n2)`.
  - If `n2 == n1`, `connecting_nodes` must be empty. This is the identity proof.
  - It is invalid if `n2 < n1`, or if `n2 == n1` but there are nodes, or if `n2 > 2^48`.
  - When `n1 == 0`, `connecting_nodes` is just the MMR peaks at `n2` highest first. Otherwise it
    follows the ancestry proof from `mmr_lib` 0.8.3, with the peaks to the right of the old frontier
    bagged into a single node (see `MmrExtensionProof::calculate_root`).

## 3. JAM Parachain Service Integration

### Host Calls

During candidate validation in Refine, validation code declares messaging roots to the Parachain
Service via two host calls:

1. `set_messages_streams_root(root)` (index 204) declares the `StreamsRoot` published by the
   candidate.
2. `set_messages_requires_roots(entries)` (index 205) declares the `(ParaId, StreamsRoot)` pairs
   consumed by the candidate.

Refine fails if either call is made twice, or if `entries` in call 205 contain more than 32 sources
(`MAX_REQUIRES_SOURCES`), contains a duplicate `ParaId` or includes the candidate's own `ParaId`.

### Settlement Ring

During Accumulate, JAM pushes each enacted `StreamsRoot` into that parachain's settlement ring,
which holds its 64 most recently inserted distinct roots.

Accumulate rejects a candidate unless every `(ParaId, StreamsRoot)` pair declared via host call 205
exists in the source parachain's settlement ring at accumulation time.

## 4. Channel Protocol and Lifecycle

### Data Types

```rust
enum ChannelMessage {
    Hello { session: u32, version: u8 }, // index 0
    CloseChannel { session: u32 },       // index 1
    Data(Vec<u8>),                       // index 2 (only payload type in channel v1)
}

struct Register {
    up_to: MessagePosition,
    status: RegisterStatus,
}

enum RegisterStatus {
    Accepting { session: u32, min: u8, max: u8, max_message_size: u32 },
    Opened,
    Closed { session: u32 },
}
```

- Channel leaves are SCALE-encoded `ChannelMessage` values and register leaves are SCALE-encoded
  `Register` values. Trailing bytes are forbidden.
- `up_to` indicates that receiver B has consumed all channel leaves strictly below that position.
- Encoded leaves cannot exceed 102,400 bytes.
- Positions, cursors and session numbers only move forward, except when a chain is forced to reset.
- If a chain loses channel state (e.g., it restarts from a new genesis), it must never reopen or
  accept the old `num`. Its peer closes the stale channel and both sides migrate to a new `num`.

### XCM Binding on Domain 0

- Only `domain 0` channels execute payloads as XCM. On sender A, only the XCM router writes to
  `domain 0`.
- Sender A routes XCM to receiver B over the `domain 0` channel with the highest `num` that meets:
  1. A wants the channel open.
  2. The most recent register A has applied is `Opened`.
  3. A has not sent `CloseChannel` for the current session.
- Each `ChannelMessage::Data` leaf holds one SCALE-encoded `VersionedXcm<()>`. A encodes it with its
  `SafeXcmVersion` until version discovery (requiring a return route from B to A) negotiates B's
  supported version.
- Receiver B executes every `Data` leaf received on any accepted `domain 0` channel with origin
  `(1, Parachain(A))`. B doesn't parse the XCM when admitting the inherent, so malformed XCM is
  still consumed and dealt with at execution.
- XCM message order is guaranteed within a single channel `num`.

### Lifecycle

```
            accept               Hello{s,v}                close_inbound()
  (none) ──────────▶ Accepting ─────────────▶ Opened ────────────────────▶ Draining
                      │  ▲  ↺ accept                │                          │
     close_inbound()  │  │ accept     CloseChannel{s}            CloseChannel{s}
                      │  │                          │         or close_inbound()
                      ▼  │                          ▼                          ▼
                     Closed ◀───────────────────────┴──────────────────────────┘
```

All channel management extrinsics require a runtime-configured privileged origin. Channel state
entries are never deleted from storage.

### Receiver State Transitions

Here `s` is B's current session. A new `accept_open_channel` bumps it to `s + 1`. Older means any
session below `s`.

| Current State | Event / Leaf | Condition | Next State | Effect |
|---|---|---|---|---|
| `none`, `Accepting`, `Closed` | `accept_open_channel` call | — | `Accepting { s + 1 }` | Publishes updated register |
| `Accepting` | `Hello { s, v }` | `v ∈ min..=max` | `Opened(v)` | Publishes `Opened` register |
| `Accepting` | `Data` | Any size | `Accepting` | Consumed and ignored |
| `Accepting` | `close_inbound` call | — | `Closed { s }` | Publishes `Closed { s }` register |
| `Opened` | `close_inbound` call | — | `Draining` | Publishes a `Closed { s }` register and keeps consuming until A closes |
| `Draining` | `close_inbound` call | — | `Closed { s }` | Drops any `Data` from session `s` that hasn't been consumed yet |
| `Opened`, `Draining` | `Data` | `len <= max_message_size` | Unchanged | Consumed and queued for execution |
| `Opened`, `Draining` | `CloseChannel { s }` | — | `Closed { s }` | Stops consuming. Any later leaves in the same inherent item are inadmissible|
| Any watched | Older `Hello` or `CloseChannel` | Session `< s` | Unchanged | Consumed and ignored |
| Any watched | Any other leaf | — | Unchanged | **Protocol violation.** The leaf is inadmissible and the channel halts until B upgrades |
| Any | Any other call | — |

- B lists a channel in `consumed_streams()` whenever the channel is `Accepting`, `Opened` or
  `Draining` and its inbound queue is under `MaxQueueFootprint`.
- Every time B's register state changes, B publishes
  `Register { up_to: frontier.leaf_count, status }`.

### Sender

- A wants a channel from the moment it calls `open()` until it calls `close()`, reading B's
  register. After `close()`, A keeps tracking `outbound_up_to()` until `up_to == sent`.
- A's current session `s` is the session from the latest `Accepting` or `Closed` register it applied
  (or `0` if none).
- `sent` is the total number of leaves A has appended to a channel.
- A applies a `Register` leaf if it decodes and satisfies the following:
  1. `up_to` does not decrease and `up_to <= sent`.
  2. An `Accepting` register has `min <= max`, and either its session is newer than A's `s` or it
     matches the `Accepting` parameters A already has stored.
  3. A `Closed` register has a session `>= s`.
  4. An `Opened` register only comes after A has sent `Hello { s, .. }`.
- A stops sending `Hello` or `Data` when `sent - up_to >= MaxInFlight`. The `CloseChannel` isn't
  held back by this rule.
- A sends `Hello { s, v }` once per session when it calls `open()`, when it applies a register, and
  when a runtime upgrade changes the versions it supports provided the following:
  - A wants the channel
  - The latest applied register is `Accepting { s }`
  - A has not yet sent `Hello { s, .. }`
  - Both sides share at least one supported version
  - The sending has capacity
- A can only send `Data` when the latest register it applied is `Opened`, it hasn't sent
  `CloseChannel { s }`, and the encoded leaf is no bigger than `min(max_message_size, 102,400)`
  bytes
- When A calls `close()` or applies `Closed { s }`, if A's last handshake was `Hello { s, .. }`, A
  sends `CloseChannel { s }` once.

## 5. Parachain Runtime

### Runtime API (`SpecMsgApi` v1)

Defined in `cumulus-primitives-core`. Runtimes that do not expose this API have offchain messaging
disabled.

```rust
trait SpecMsgApi {
  fn outbound_messages() -> Vec<(StreamId, Vec<Payload>)>;
  fn consumed_streams() -> ConsumedStreams;
  fn max_touched_streams() -> u32;
  fn consumption_record() -> ConsumptionRecord;
  fn outbound_up_to() -> BTreeMap<OutChannelId, MessagePosition>;
}

struct ConsumedStreams {
    channels: BTreeMap<InChannelId, MmrFrontier>,       // cursor = frontier.leaf_count
    registers: BTreeMap<OutChannelId, MessagePosition>, // cursor = last read position + 1, or 0
    events: BTreeMap<BroadcastId, MessagePosition>,     // reserved (empty)
}

struct ConsumptionRecord {
    entries: BTreeMap<ParaId, Vec<(StreamId, Interval)>>, // sorted and unique
}

struct Interval {
    start: MmrRoot,
    end: MmrFrontier,
}
```

- `outbound_messages()` returns every stream written during the block in key order, with payloads in
  position order.
- `outbound_up_to()` exposes the latest applied `up_to` per outbound channel so archive nodes can
  prune acknowledged payloads at finalized blocks while keeping the frontier to serve proofs.

### Local Pallet Constants

| Constant | Meaning |
|---|---|
| `MaxInFlight` | Maximum unacknowledged leaves (`sent - up_to`) per outbound channel. |
| `MaxQueueFootprint` | Maximum ready pages in an inbound channel's message queue. |
| `MaxTouchedStreams` | Maximum streams a single block's inherent may read. |
| `MaxInboundMessagesPerItem` | Maximum payloads allowed in a single inherent stream entry. |


### Outbound Finalization (`on_finalize`)

1. While the block runs, `send` adds each payload to a pending list kept in memory. A payload's
   position is `frontier.leaf_count + pending_index`.
2. In `on_finalize`, the pallet:
  - publishes any changed registers
  - computes updated `MmrRoot`s
  - updates the `StreamsRoot` trie
  - emits the `SPMS` consensus digest
  - updates stored frontiers
  - populates `outbound_messages()`.

3. Callers pay the weight for their append and trie path updates. Any runtime-level per-block send
   cap must exclude sends triggered by the messaging inherent and register publications.

### Messaging Inherent (`specmsg0`)

```rust
struct MessagingInherentData {
    channels: BTreeMap<InChannelId, Vec<Payload>>,  // starting from cursor
    registers: BTreeMap<OutChannelId, LossyRead>,   // exactly 1 Register payload
    events: BTreeMap<BroadcastId, LossyRead>,       // must be empty
}

struct LossyRead {
    frontier: MmrFrontier,
    payloads: Vec<Payload>,
}
```

- This is a Mandatory inherent and must be included at most once per block. It is omitted if there
  is nothing to consume. Every `BTreeMap` must decode with strictly increasing keys.
- An item is valid only if:
  - It is listed in `consumed_streams()` and is non-empty
  - It stays within `MaxTouchedStreams`, `MaxInboundMessagesPerItem`, `MaxQueueFootprint` and 32
    sources per block
  - Its channel leaves follow the receiver B transition table
  - Register reads start at or above the stream's cursor
- Admission checks read only the messaging pallet's storage and queue ready pages. Earlier inherents
  must not modify these so block building and block import reach identical admission decisions.

### Weight and PoV Budgeting


- After `on_initialize`, the runtime calculates the inherent's budget for both `ref_time` and
  `proof_size`. It starts from the block's max weight and subtracts the weight already used, the
  reserves for other inherents, and `MaxServiceWeight` if a multi-block migration is running. FRAME
  does not cap the weight of Mandatory inherents, so import rejects any block that goes over this
  budget.
- The inherent's weight covers one register publish per inbound channel item and one send per
  register read. Its encoded bytes count toward the PoV size.
- `create_inherent` truncates each item just before its first inadmissible leaf and drops any items
  left empty. It then trims what's left to fit the weight budget and the remaining Mandatory block
  length.
- The runtime's maximum block `proof_size` must leave enough room under the JAM work-package limit
  for the lifts of `MaxTouchedStreams` streams and for resubmission data.

### Refine Validation (`jam_validate_block`)

The validation code runs the candidate's block in Refine:
- It reads the SPMS digest and consumption record during execution.
- After execution:
  - The validation code calls `set_messages_streams_root` with the digest's root if present
  - If block consumed messages, it decodes the PoV's lifts as
    `Vec<(ParaId, Vec<RequiresLift>)>` with sources strictly increasing and one lift per recorded
    stream
- For each `RequiresLift { advances, extension, tree_proof }`:
   - Verify `advances` is empty.
   - Apply `extension` to the stream's recorded interval `end` frontier, then walk `tree_proof`
     using the stream key.
   - Verify that all streams from the same source `ParaId` resolve to the exact same `StreamsRoot`,
     then pass all `(ParaId, StreamsRoot)` pairs to `set_messages_requires_roots` (205).
- Any decoding error, missing/extra lift, non-empty `advances`, or failed proof check causes Refine
  to panic without writing a log entry

## 6. Fetch Protocol

- Messages are fetched from the request-response protocol on `/{jam_genesis_hash}/spec-msg/fetch/1`.
- Receiving nodes dial sender nodes directly without opening block-announce substreams
- Requesters look up the sender's bootnode record under `bootnodes/v1` in the Parachain Service
  key-value store and use a static list if provided via `--spec-msg-peer <para_id>=<multiaddr>`
  flags. 

### Request and Response

Malformed requests without exact decoding receive no response.

```rust
struct MessagesRequest {
    stream: StreamId,               // a stream of the serving chain
    under: StreamsRoot,
    start: Option<MessagePosition>, // first leaf wanted, None for the head
    max_bytes: u32,
}

struct MessagesResponse {
    frontier: MmrFrontier,
    payloads: BoundedVec<Vec<u8>, ConstU32<1024>>,
    extension: MmrExtensionProof,
    tree_proof: StreamProof,
}

type FetchResponse = Result<MessagesResponse, FetchError>;

enum FetchError {
    UnknownStream,
    UnknownRoot,
    OutsideHorizon { lowest: MessagePosition },
    BeyondHead,
}
```

### Server Behavior

Let `N` be the stream's `leaf_count` under `under`:

1. Return `UnknownRoot` if `under` is unknown/pruned, or `UnknownStream` if `stream` is absent from
   the trie.
2. Let `s = start.unwrap_or(N - 1)`. Return `BeyondHead` if `s > N`, or `OutsideHorizon { lowest }`
   if `s` has been pruned.
3. Return `frontier` at `s` and sequential payloads starting at `s` (empty if `max_bytes == 0`),
   stopping at `N`, `1,024` payloads, `max_bytes`, or a total encoded response size of 16 MiB.
4. Attach `extension` (from `s + payloads.len()` to `N`) and `tree_proof`.

### Requester

A response is accepted only if:
1. It decodes within the limits and every payload is at most 102,400 B.
2. When `start == Some(s)`, `frontier.leaf_count == s`. If the requester knows the local frontier at
   `s` (the channel cursor or recorded interval `end`), `frontier` must match it.
3. Total payload bytes are `<= max_bytes` (and `0` when `max_bytes == 0`).
4. Appending `payloads` to `frontier`, applying `extension`, and walking `tree_proof` with `stream`
   yields `under`.
5. For head requests (`start == None`), `extension.leaf_count == frontier.leaf_count + 1`. This
   stops a stale leaf from being used as the stream head.
6. If `payloads` stops before `extension.leaf_count` with fewer than 1,024 items, then
   `min(max_bytes, 15 MiB) - payload_bytes < 102,400`. This makes sure servers fill their responses,
   while leaving 1 MiB for proofs.

If a request fails or times out, the requester queries another peer. Failing only check 2's frontier
equality check ends the request without penalizing the peer, since check 4 already proves
authenticity under `under`.

### Node Archiving

- Serving nodes archive `outbound_messages()` for every imported block across all forks, verifying
  the recomputed `StreamsRoot` against the block's `SPMS` digest. Nodes serve verified blocks
  immediately without waiting for JAM accumulation.
- If a node lacks historical data (e.g., after warp sync), it fetches missing leaves from peer
  servers on its own chain, verifies them against its own digests, and responds with
  `OutsideHorizon` until caught up.
- Archive nodes must retain:
  - All roots in their chain's settlement ring at the latest finalized JAM block, plus all
    `StreamsRoot`s for archived blocks above finalized height (and trie nodes needed to serve
    `tree_proof`s for each).
  - Every channel leaf at or above `outbound_up_to()` at the latest finalized block.
  - Every register leaf at or above the stream head under the oldest retained root.
  - MMR frontiers at all retained positions and connecting nodes to each retained root.
- Any storage layout is valid provided it serves `k` payloads in `O(k + log N)` time.


## 7. Receiver Collator Workflow

1. **Fetch:** At the parent block, read `consumed_streams()`. For every active source that has a
   settlement ring at the JAM anchor, fetch under its newest ring root:
   - inbound channels, starting from `cursor`, with `max_bytes >= 512 KiB`, and
   - outbound channel registers, as head requests (`start: None`) with `max_bytes = 102,400`.
2. **Select `TargetRoot`:** Wait for in-flight fetches for up to a configurable window (**250 ms**
   by default). Then, for each source, pick a `TargetRoot` from the **newest 48 roots** in its
   settlement ring at the anchor. Prefer the root with verified responses for the most streams, and
   if there's a tie, take the newest one.
3. **Build Block:** Only payloads verified under each source's `TargetRoot` go into the inherent
   data, trimmed as described in Appendix B. Responses that arrive late can be cached for later
   candidates, as long as their root is still among the newest 48 ring entries.
4. **Build Lifts:** Read the built block's `consumption_record()`. For each stream it touched, build
   a `RequiresLift` from the interval's `end` to `TargetRoot`. Do this by rebasing the fetch
   response's `MmrExtensionProof` over any tail leaves that weren't consumed, using
   `MmrExtensionProof::rebase`. Never submit a candidate unless every lift is there.
5. **Unaccumulated Parents:** Collators can build on a parent block that JAM hasn't accumulated yet,
   and they don't need the parent's lifts to do it. They must not drop blocks when a channel fetch
   fails check 2. Otherwise, a source that rewrites its history could fork B every time the author
   rotates.
6. **Resubmissions:** When resubmitting an unaccumulated block in a new work package, keep the block
   and pick a `TargetRoot` at the new anchor. Keep any existing lifts that already end at that
   `TargetRoot`. For any lift that's missing or doesn't match, send zero-payload lift requests
   (`{ start: Some(end.leaf_count), max_bytes: 0, under: TargetRoot }`) before the normal fetches.
   If lifts are still missing, or the block's anchor or lookup anchor can't be proved anymore, drop
   the block and everything built on it.


### Failure Matrix

| Failure Condition | Detected By | Action | Impact |
|---|---|---|---|
| Fetch error, timeout, or failed verification | Requester | Query another peer | Latency |
| Sender stream reset or rewritten below cursor | Collator | Stop consuming from stream | Channel halts (see below) |
| Missing lift or unprovable anchor on resubmission | Collator | Drop block and its descendants | Latency |
| Declared `TargetRoot` missing from source's ring | Accumulate | Reject candidate and descendants | Latency |
| Source parachain deregistered | Collator | Stop consuming; drop unlifted blocks | Unconsumed tail messages lost |
| Invalid channel leaf sequence/format | Receiver runtime | Reject leaf as inadmissible; stop channel | Channel halts until B upgrades |
| Host call misuse or invalid lift proof | Refine | Panic (misuse logged in `parachain_log`) | Candidate rejected |


### Known Limitations

- **Accumulation is not final:** A JAM reorg that reverts the accumulation of A's root invalidates
  receiver anchors referencing it, causing B's affected blocks to be dropped and rebuilt.
- **No import-time ring check:** B's runtime cannot read JAM's settlement ring during block import.
  A block consuming under an unaccumulated root imports and passes Refine, failing only at
  Accumulate.
- **Ring eviction window:** A work package is reported within 8 JAM blocks of its anchor and becomes
  available within 5 slots. Unless deferred due to Accumulate gas limits, it accumulates within **13
  JAM blocks**. A sender that enacts one candidate per JAM block leaves a `TargetRoot` from the
  newest 48 at worst 61st of 64 (`48 + 13 = 61`). A sender running on several cores pushes several
  roots per JAM block and can evict an older `TargetRoot` first. The receiver's candidate then
  fails the settlement check and is rebuilt, which costs latency.
- **Dead receivers:** If B stops consuming permanently, messages remain undelivered and A's channel
  eventually stalls at `MaxInFlight`.
- **Lost archives:** If all sender archive nodes lose unconsumed payloads, those messages are
  unrecoverable. B must call `close_inbound` until `Closed` and coordinate a new channel `num`.
- **Forced head resets:** `ParachainSetHead` preserves the settlement ring. If B accumulates
  consumption past A's reset point (or if A's runtime rewrites a stream), the channel halts once the
  histories diverge. Recovery requires closing the channel and opening a new `num`.
- **ParaId Reuse:** Coretime must never reassign a deregistered `ParaId` (to prevent inheriting
  stale channels/XCM origins) and must never assign IDs below `2000` to non-system chains.

## Appendix A. Constants

| Constant | Value | Section |
|---|---|---|
| Settlement ring capacity | 64 most recently inserted distinct roots | Parachain Service |
| `TargetRoot` age | Newest 48 ring roots at the anchor | Receiver collator |
| Requires sources per candidate | 32 | Parachain Service, Messaging inherent |
| Extension proof nodes | 48 | Extension proofs |
| Stream proof steps, trie path | 64 | Streams root and digest |
| Leaf and payload size | 102,400 B | Channels, Requester |
| Payloads per response | 1,024 | Server |
| Request size | 1 KiB | Transport and discovery |
| Response size | 16 MiB | Server |
| Channel fetch budget floor | 512 KiB | Receiver collator |
| Request timeout, dial included | 15 s | Transport and discovery |
| Lift size bound per stream, source, block | 3,660 B, 8 B, 2 B | Budget |
| Parachain Service id | 1337 | Reading the ring |

## Appendix B. Normative details

### Fetch Server Rate Limits

- Each incoming request is charged to the remote address of the peer's oldest open connection. An
  IPv4 address counts as a single `/48` with one `/64` inside it.
- Each `/64` can have at most 64 requests held at once and 32 MiB of total `max_bytes`. For this
  count, each request's `max_bytes` is capped at 16 MiB. Anything over the limit is dropped right
  away.
- When the server holds more than 4,096 requests in total, or the total counted `max_bytes` goes
  over 320 MiB, it drops the newest request from the busiest `/64` in the busiest `/48`, measured by
  counted `max_bytes`.
- Responses are served round-robin across `/48` prefixes, and round-robin again across the `/64`
  subnets inside each `/48`.

### Requester Peer Policy

- Requesters send each request to up to 3 peers at once and take the first response that passes
  every check.
- Each requester keeps at most 64 requests in flight and 32 MiB of counted `max_bytes` per peer.
- Requesters prefer the peers that most recently sent back a verified response. Peers that timed out
  or failed verification go to the back of the line until they succeed again. Ties are broken at
  random. If no peer responds before the collator's wait timer runs out, every peer that was asked
  is marked as timed out.

### Inherent Trim Order

When `create_inherent` has to trim items to fit the block's weight or Mandatory length limits, it
should add items in three passes. The starting key index rotates with the block number and wraps
around, so no message class or low-ID peer can starve the others:

1. Register reads, for up to `MaxTouchedStreams / 2` streams and 16 sources (half of 32).
2. Inbound channel items, up to whatever is left of the stream, source and byte budgets.
3. The remaining register reads, up to whatever budget is left.

Block import checks the final limits and admission rules, but not the order items were trimmed in.
