# Speculative Messaging on JAM: MVP

The [Parachain Service](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service) replaces HRMP-style payloads with [Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md).

This doc is the implementation scope for initial launch. Chains consume at the `Enacted` tier
(HRMP parity).The sender's root is pushed into an on-chain settlement ring when its candidate
enacts, and the receiver's candidate enacts only if every root it consumed is in the ring.

To make this happen, we need to build three core components:

1. [`Provides` and `Requires`](#1-provides-and-requires)
2. [Settlement Ring](#2-settlement-ring)
3. [Bootnode Discovery](#3-bootnode-discovery)

For a concrete breakdown of the work, see the [Implementation Tasks](#implementation-tasks).

## 1. `Provides` and `Requires`

The Parachain Service work digest gains two fields:

```rust
struct ParachainWorkDigestOk {
  /// 33 B when present.
  spec_msg_provides: Option<StreamsRoot>,
  /// 36 B each, ~1.1 KiB at full fan-in.
  spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), 32>,
}
```

Two new host calls, each callable at most once per Refine:

```rust
/// Declares the candidate's provides root. Omitted when the candidate sent nothing.
fn set_provides_root(root: StreamsRoot);

/// Declares the candidate's requires entries, one per consumed source.
/// One call carries the whole set.
fn set_requires_root(entries: &[(ParaId, StreamsRoot)]);
```

PS remains completely header-agnostic. It simply moves opaque 32-byte roots around without ever needing to decode the actual head data.

### Sender

The sender's runtime is responsible for maintaining its stream MMR frontiers and the commitment tree.
When it produces a block, it writes the `Provides` root into both the pallet storage and the header digest.

In Refine, after PVF execution, the `validate_block` wrapper reads the root from pallet
storage and reports it via `set_provides_root`. For bundled PoVs, the wrapper carries the last
produced root forward even when later inner blocks send nothing. Intermediate roots are not
settlement entries.

In Accumulate, when the candidate enacts, the root is pushed into the sender's ring
(`ring[A]`). A root enters the ring only for an enacted candidate.

### Receiver

The collator targets the latest enacted `Provides` of each source and consumes a prefix of
messages. The runtime records a `ConsumptionRecord`. The collator generates the Lift proofs
and packages them into the PoV.

In Refine, the wrapper reads the `ConsumptionRecords` from pallet storage together with the
Lifts from the PoV, stitches consumption gaps, and sets the final `Requires` via
`set_requires_root`. For a bundle, it unions the per-block records into one entry per source,
keeping the latest root. Refine enforces that consumed sources are unique and at most 32.

In Accumulate, settlement checks that each declared `(ParaId, StreamsRoot)` is present in
`ring[ParaId]`. The candidate enacts only if every entry settles. Otherwise, it is rejected.
At the Enacted tier a rejection only happens when the target root has already fallen out of
the 64-entry window. This is mitigated with receiver monitoring and resubmissions, and should be
rare in practice with a 64-entry window and 3 parachain cores.

## 2. Settlement Ring

The ring holds the last `W_MAX = 64` enacted roots per para. Maximum footprint is 9747 B.

```rust
/// Tracks order and capacity.
///
/// Key: `0x09 ++ para_id`
///
/// Billed: 47 B (never read during settlement)
spec_msg_cursor: Map<ParaId, Cursor {
  /// Sequence of next push (wrapping).
  head: u32,
  /// Sequence of the oldest entry.
  tail: u32,
}>

/// Maps the sequence to the streams root.
///
/// Key: `0x0b ++ para_id ++ seq`
///
/// Billed: 75 B (Read on capacity eviction and lifecycle).
spec_msg_queue: Map<(ParaId, u32), StreamsRoot>

/// Ensures the `StreamsRoot` is present in the ring.
///
/// Key: `0x0a ++ para_id ++ root`
///
/// Billed: 75 B (Read only by settlement check).
spec_msg_member: Map<(ParaId, StreamsRoot), MemberEntry {
  /// The queue position ensuring duplicate guard and consumer hints about
  /// live chain sets.
  seq: u32
}>
```

The settlement check is a single 75 B point read of `spec_msg_member` per `Requires` entry.

When a new root is added, the system drops the oldest tail entry if the queue is full
(unless a recent duplicate exists), inserts the new root at the head, logs its position,
and advances the cursor to save the state.

Eviction from the ring is based entirely on position. A root is only pushed out after the parachain adds 64 newer roots.
If a block sends nothing, nothing is pushed. If a root happens to be repeated, the `MemberEntry` simply shifts to the
new position while the old queue slot is left behind.

**Teardown.** When `parachain_set_head` overwrites a live head, or `parachain_clean_up` is
called, the ring is cleared. The ring is bounded by `W_MAX`, so teardown is bounded too
(64 queue reads, 128 deletes, one cursor read and one cursor delete). 
The Coretime call's gas budget must cover the full walk, since teardown is a fixed cost, not a failure mode.

## 3. Bootnode Discovery

Receivers fetch messages over the request-response `/spec-msg/exchange` protocol (same as the
v0.5 SpecMsg design), so a node must discover the source para's peers first. Each para
publishes its bootnodes in the existing para-owned KV store (tag `0x08`):

```rust
/// Full storage key: 0x08 ++ SCALE((para_id, BOOTNODES_KEY)).
const BOOTNODES_KEY: &[u8] = b"bootnodes/v1";

/// Ephemeral record of a parachain's active bootnodes.
struct AddressRecord {
    /// The timeslot after which this entire record is invalid and should be dropped.
    ///
    /// `expires_at` is the runtime's liveness statement for the whole record.
    expires_at: Timeslot,
    /// Up to 4 active bootnodes.
    bootnodes: BoundedVec<Bootnode, 4>,
}

struct Bootnode {
    /// The network address (must NOT contain a peer ID).
    addr: Multiaddr,
    /// The public key from which the node's peer ID is derived.
    node_key: Ed25519Public,
    /// A monotonic sequence number to prevent replay attacks.
    seq: u64,
    /// Proof of possession. The node must sign:
    /// `("bootnode-pop-v1", jam_genesis_hash, para_id from the key, Bootnode::seq, Bootnode::addr)`
    /// This proves the node owner consents to being a bootnode for this specific broadcast.
    sig: Ed25519Signature,
}
```

PS does not verify the `AddressRecord` fields and admission into the record is parachain runtime
logic. KV writes are charged and skipped on insufficient balance.

The parachain runtime handles the rest of the bootnode lifecycle.
The pallet maintains the active bootnode set in its own storage, indexing entries by `node_key`.
Modifying this set requires administrative privileges, the `add_bootnode` and `remove_bootnode` extrinsics
are restricted to `AdminOrigin` (which defaults to governance). The proof of posession ensures the
node's consent to be listed.

During admission, the runtime verifies the `sig` and requires the `seq` to exceed the stored `seq` for that `node_key`.
When a change is accepted, the pallet reencodes the full record and emits a new one via `kv_set`.
The record is refreshed before `expires_at` to ensure it remains valid.

During the initial chain setup, the operator provides an explicit `--bootnodes` CLI flag.
Once that record is published to the chain, any new collators and nodes can easily join by reading it.

## End to End

1. A's runtime appends outbound messages to per-destination stream MMRs and writes the
   `StreamsRoot` to pallet storage.
2. A's Refine reports the root via `set_provides_root`. A's Accumulate enacts the head and
   pushes the root into `ring[A]`.
3. B's node follows A's enacted heads, fetches the messages over `/spec-msg/exchange`, and
   verifies them against A's `StreamsRoot`.
4. B authors a block consuming stream prefixes. B's Refine verifies the PoV Lifts and sets
   `Requires`. B's Accumulate settles each entry against the ring and enacts.

## Implementation Tasks

This list only covers work that is entirely new or requires changes specifically for JAM.

The existing parachain-side stack from the Polkadot rollout (primitives, spec-messaging pallets,
channel lifecycle, XCM routing, `/spec-msg/exchange`, archive, fetcher, message pool, and
lift assembly) carries over unchanged.

**Umbrella 1 — Parachain Service**

- [Task 00] Work digest and host calls: Implement `spec_msg_provides` and `spec_msg_requires`
  fields in `ParachainWorkDigestOk`.  
  Add `set_provides_root` and `set_requires_root` host calls.  
  Enforce Refine rules: max one call each, unique sources, and a 32-entry maximum.
- [Task 01] Settlement ring: `spec_msg_cursor` / `spec_msg_queue` / `spec_msg_member` storage.  
  Push on candidate enactment with position-based eviction and re-push handling.
- [Task 02] Settlement check: Per-`Requires` membership read in Accumulate.  
  Candidate rejected when any entry misses.
- [Task 03] Ring teardown: Clear the ring in `parachain_set_head` (live-head overwrite) and
  `parachain_clean_up`.  
  Walk `head - 1` down to `tail`, Coretime provides sufficient gas budget for the full walk.

**Umbrella 2 — `validate_block` wrapper**

- [Task 04] Provides reporting: After PVF execution read the root from pallet storage and
  report it via `set_provides_root`.  
  Carry the last produced root forward across bundle blocks.
- [Task 05] Requires synthesis: Read `ConsumptionRecords` from pallet storage, verify the PoV
  lifts, stitch consumption gaps, union per source for bundles, report via
  `set_requires_root`.  

**Umbrella 3 — Bootnode discovery**

- [Task 06] Runtime publishing: Maintain the `AddressRecord` under KV tag `0x08`
  (`bootnodes/v1`): admission logic, `seq` bumping, expiry. The whole record is republished on every change
  and is refreshed before `expires_at`.
- [Task 07] Node side: Proof-of-possession signing and verification, reader side checks (PoP, expiry, highest
  `seq`), record reading to discover a source para's peers, `--bootnodes` CLI flag for initial setup, feeding the peer
  registry.

**Umbrella 4 — Node adaptations**

- [Task 08] Provides monitor on JAM: Follow PS enacted state instead of the relay
  `RecentProvides` key.  
  Read the source's ring, emit exactly-once per-root events into the existing fetch pipeline.
- [Task 09] Prefetch from guaranteed reports: Read `spec_msg_provides` out of the guarantees
  extrinsic of imported JAM blocks, before the report accumulates, and fetch the messages
  under that root over `/spec-msg/exchange` right away.   
  Hand them to the pool only once the monitor sees the root in the ring, since a guaranteed
  report can still time out or be disputed.  
  Saves network lantency and needs no consensus change.
- [Task 10] Eviction watch and resubmit: For every candidate submitted but not yet enacted,
  track its `Requires` roots against the source ring (`MemberEntry.seq` vs the cursor tail).  
  When a root falls out of the 64-entry window, regenerate the lifts against the source's
  latest enacted root and resubmit.  
  The consumed prefix is unchanged since the MMR only grows.
- [Task 11] Collator wiring: Wire monitor, fetcher, pool and lift assembly into the JAM
  parachain collator node.

**Umbrella 5 — End-to-end gate**

- [Task 12] JAM end-to-end test: Two paras on a local JAM network: sender push into the ring,
  receiver settlement and enactment, rejection on window eviction, ring teardown.  
  Gates any testnet rollout.
