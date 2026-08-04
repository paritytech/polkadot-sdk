# Speculative Messaging on JAM

The [Parachain Service](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service) needs a robust messaging system to replace the older HRMP-style payloads, and [Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md) is our primary candidate for JAM.

Here is a breakdown of exactly what needs to change to make Speculative Messaging work smoothly on JAM, ensuring every Polkadot feature has a clear, explicit JAM backport.

## 1. Consensus and State Changes

### 1.1 Digest Fields Replace UMP Signals

On Polkadot, `Provides` and `Requires` travel as [UMP signals](https://github.com/paritytech/polkadot-sdk/blob/436cd3373a71f1c2251ce3976ee04220cf2646be/polkadot/primitives/src/v9/mod.rs#L2740-L2760).

In JAM, the consensus output per candidate is `ParachainWorkDigest::Ok`, which is assembled by the refine wrapper.

We need to add two new digest fields (optionally present and at most once per child PVM via `set_provides_root` and `set_requires`).
If a duplicate call is made, Refine fails. At 64 sources, this takes up about 2.3 KiB, which is a comfortable 5% of our 48 KiB work-report budget.

```rust
const MAX_REQUIRES_SOURCES: u32 = 64;

struct SpecMessagingDigest {
    provides: Option<StreamsRoot>,
    requires: BoundedVec<(ParaId, StreamsRoot), MAX_REQUIRES_SOURCES>,
}

enum ParachainWorkDigest {
    Ok {
        para_id: ParaId,
        // .. existing variants
        spec_messaging: SpecMessagingDigest,
    }
}
```

### 1.2 The `RecentProvides` Service State

To perform the settlement check during the `Accumulate` phase, the Parachain Service needs to maintain a ring of [RecentProvides](https://github.com/paritytech/polkadot-sdk/blob/436cd3373a71f1c2251ce3976ee04220cf2646be/polkadot/runtime/parachains/src/spec_msg.rs#L55-L70) roots for every source parachain.

This is a new service [state item](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#61-state-balance-accounting) (keyed under tag 0x08) that is written exclusively by `Accumulate` upon enactment. It has a fixed worst-case footprint of 4,649 bytes, which is folded directly into the per-para [baseline footprint](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#sizing-the-baseline-footprint).

> **Important**: The [parachain_clean_up](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#side-effect-host-functions) must additionally drop the ring entry when a parachain is offboarded. Otherwise a reused `ParaId` inherits the prior stale roots.

```rust
pub const RECENT_PROVIDES_WINDOW: u32 = 128;
pub struct RecentRoots(BoundedVec<StreamsRoot, ConstU32<RECENT_PROVIDES_WINDOW>>);

// Keyed under tag 0x08. Written only by Accumulate.
recent_provides: Map<ParaId, RecentRoots>
```

### 1.3 Settlement in Accumulate

Polkadot handles settlement by dropping [candidates pre-inclusion](https://github.com/paritytech/polkadot-sdk/blob/436cd3373a71f1c2251ce3976ee04220cf2646be/polkadot/runtime/parachains/src/paras_inherent/mod.rs#L1043-L1058) if Requires doesn't match Provides. Because JAM lacks a pre-inclusion hook, this check shifts into the Accumulate phase (specifically between the validation-code check and the head-data update).

Every required root in the digest must match the source's recent_provides state. If there's a miss, the candidate is rejected, the receiver burns B's core slot (a difference from Polkadot), and it logs an `AccumulateLog::RequiresUnmet` error. The collator then simply drops the package, rebuilds, and retries.

```rust
5. Validation code check: ..
6. Speculative Messaging Settlement
7. Head-data update: ..
```

[AccumulateLog](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#31-service-state-layout) is adjusted to reflect the new settlement check:

```rust
enum AccumulateLog {
    // .. existing variants
    RequiresUnmet { src: ParaId, root: StreamsRoot },
}
```

## 2. Transport And Discovery

### 2.1 Transport Payload

For the MVP, parachains will exchange messages strictly offchain via peer-to-peer (P2P) using a `/spec-msg/exchange` request-response protocol. The payloads remain in the sender's [node side archive](https://github.com/paritytech/polkadot-sdk/blob/436cd3373a71f1c2251ce3976ee04220cf2646be/cumulus/client/spec-msg/src/archive.rs#L18-L57) until the receiver acknowledges them.

Future Extension: if P2P isn't available, the JAM-native fallback will be exporting payloads into the Data Availability (DA) lake, allowing receivers to fetch them directly from DA segments.

### 2.2 The Discovery Mechanism

JAM doesn't have a DHT, meaning we lose Polkadot's "Bootnodes on DHT" feature. To allow parachains to find each other and fetch messages, the parachain service introduces an authorship-derived bootnode ring.

Collators will attach a publicly reachable `Multiaddr` to their work items. The refine wrapper forwards this to Accumulate, which rotates it into a `recent_collator_addr`s` map for accepted candidates. The receiving parachain uses this ring to bootstrap the DHT and fetch payloads directly.

```rust
pub const MAX_COLLATOR_ADDR_RING: u32 = 20;
struct CollatorAddrRing(
    BoundedVec<(Timeslot, CollatorKeyHash, Multiaddr), MAX_COLLATOR_ADDR_RING>
);

/// Discovery anchor, keyed under tag 0x09.
recent_collator_addrs: Map<ParaId, CollatorAddrRing>
```

**Extrinsic Payload**

The [work item](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#32-work-items) is naturally extended to carry the collator provided multiaddress:

```rust
 struct WorkItemExtrinsics {
        // { validation_code_hash, pov }
        candidate: ParachainCandidate,
        /// The serving endpoint shouldn't be the authoring node's IP,
        /// but a bootnode into the DHT of the parachain.
        recent_collator_addr: Option<Multiaddr>,
  }
```

During the refine step, this address is forwarded directly to the Accumulate phase via the `SpecMessagingDigest` (see 1).

```rust
struct SpecMessagingDigest {
    provides: Option<StreamsRoot>,
    requires: BoundedVec<(ParaId, StreamsRoot), MAX_REQUIRES_SOURCES>,

    /// Newly added collator address, forwarded exactly by the refine step.
    collator_addr: Option<Multiaddr>,
}
```

**Service State Authoring ring**

To maintain recent discovery addresses, the service state implements an LRU-based authoring ring (stored as a map under discovery anchor tag `0x09`).
- Bounded Capacity: Limited to a maximum of 20 entries per parachain (similar to DHT bootnode limits).
- Anti-Spam: Entries are keyed by `CollatorKeyHash`. This ensures a malicious collator can only occupy a single slot in the ring.
- Temporal Decay: Entries are timestamped. The consumer-side enforces a soft limit, ignoring addresses older than 2 hours.
- Performance: Uses an LRU eviction policy (requiring a mem-move of a few KiB to shift the vector).


**State Transition**

```rust
// ..
// 6. Speculative Messaging Settlement
// 7. Head-data update: ..
// (new) 8. Update collator ring address.

    if let Some(addr) = digest.collator_addr {
        let author = collator_key_from_authorizer_trace(&report)
                .unwrap_or(CollatorKeyHash::LATEST_AUTHOR);
        recent_collator_addrs[para_id].touch(timeslot, author, addr);
    }
```

> **Important**: The [parachain_clean_up](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#side-effect-host-functions) needs to clean up the ring.

**Node side contract**
- Addresses outside the 2-hour window are strictly ignored.
- Address deduplication is handled defensively at dial time.
- The consumer node is responsible for protecting itself against invalid or malicious dials.


**Storage overhead**
- The `Multiaddr` is capped to 128B.
- Worst-case footprint for `recent_collator_addrs` is `3360 B` at 128-B addresses
- This gets folded into the per-parachain baseline_footprint, increasing it from `74,474` bytes to `77,834` bytes (including the provides ring).


## 3. Execution and Verification

### 3.1 Receiver Verification

The child PVM has no crypto host calls, and the BLAKE2b-256 hash function runs in guest code. The `sp_io`-on-PVM guest shim provides hashing, crypto, and allocators in-guest via a target-generic `replace_implementation` mechanism.

1. The receiver fetches payload bytes and verifies them.
2. The PVF recomputes the hash tree using BLAKE2b-256 inside the guest code.
3. The wrapper synthesizes the `Requires` field.
4. Accumulate matches these roots against the `RecentProvides` ring.

### 3.2 Lifts in PoV & Handshakes

**Lifts**

The PoV carries lifts using the [ParachainBlockData::V3](https://github.com/paritytech/polkadot-sdk/blob/436cd3373a71f1c2251ce3976ee04220cf2646be/cumulus/primitives/core/src/parachain_block_data.rs#L88-L102) framing. Lifts are bounded at ~512 KiB worst case (64 sources * a pessimistic 8 KiB of proof material), against ~13 MB of PoV headroom on JAM.

The relay shaped `scheduling_proof` field rides along in V3 and is pinned `None` in the JAM.

Only 36 B per source (the digest's `(ParaId, StreamsRoot)` pair is within JAM's on-chain budget of the 48 KiB per work report). The lift proofs stay in the PoV, which is consumed in-core and never lands on-chain.

**Handshake**

Channel handshakes (`open_channel` / `accept_open_channel`) port over perfectly.

The JAM parachain service doesn't need host functions for this. It's handled entirely on the parachains side.

The same holds for flow control: registers, credit windows, all in-band and stream-ordered, riding the same streams as the data.

## 4. Node Stack

The node side requires a new `ProvidesSource` trait to interface with the messaging pipeline. This trait must expose the following capabilities: block stream, startup tip, sync gate, ring read at a recent block hash, pending-provides hint for the guaranteed tier. 

A newly introduced JAM `ProvidesSource` will be responsible for extracting the latest provides. It accomplishes this by reading the tag derived key `0x08 ++ SCALE(para_id)` at imported JAM blocks.

## 5. Latency Tiers

The messaging architecture is designed around progressive latency tiers, establishing a robust baseline at launch while paving the way for highly optimized pipelines post MVP.

**MVP**
- Accumulate Tier: The root becomes visible in recent_provides immediately after the Accumulate step. This is functionally equivalent to the inclusion tier on Polkadot.
- From launch, message latency will maintain parity with Polkadot's existing HRMP performance.

**Post MVP**
- JAM Backed Tier: Work reports become available approximately 1–2 timeslots before the Accumulate step.
  - Initially deployed as a prefetch hint. Once Accumulate survival rates exceed 99% within a 2-slot window, it can be shifted into active use.

- WIP In-Core Import Tier: Parachain B's work package explicitly declares Parachain A's exported payload segments as imports. This creates a tight coupling between B's block production and A's segment export timing and Data Availability (DA) window.

- BestBlock Tier: Parachain B acts proactively on Parachain A's announced "best block" before any data lands on-chain (mirroring current Polkadot behavior). This tier may require the future implementation of a dedicated `/spec-msg/announce` protocol or similar.


## 6. Primitives

- One single crate under `cumulus/primitives/spec-messaging` (StreamId, MMR, tree, lifts, wire, fixtures) compiles for `riscv64emac-unknown-none-polkavm` in CI from the moment JAM work starts.
- Pallets are ported directly via [cumulus on JAM plan](https://github.com/paritytech/polkadot-sdk/blob/mku-cumulus-on-jam-doc/designs/cumulus-on-jam/cumulus-on-jam.md).

**Migration**

- No migration to tackle on JAM, implementation lands from day zero. The XCM router is simply `SpecMsgRouter` without HRMP part.

### Open Questions
- PVM blake2b throughput needs benchmarks. Fallback is coordinated via leaf version
- DA segment for messages vs P2P only for recovering messages for MVP
- `MAX_REQUIRES_SOURCES` bounded at 64, needs more data for a bump
- Exposed JAM `ProvidesSource` unverified

