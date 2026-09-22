# Speculative Messaging: Consensus-Critical Encoding Specification

Companion to [speculative-messaging-design.md](speculative-messaging-design.md) v0.5. This document
pins every byte that must be bit-identical across all implementations. Unmarked sections are
normative and implemented in `cumulus-primitives-spec-messaging` / `polkadot-primitives::v9`.
Items marked **⚠ OPEN** are listed in §13.

Conventions: `H(x)` = BLAKE2b-256 (`SpecHasher`); `Hash` = 32 bytes; `‖` = byte concatenation;
SCALE unless stated. Integer fields inside the 8-byte `StreamId` are big-endian so the encoding
sorts like the field tuple.

---

## 1. Hash domain tags

One byte, always the first preimage byte. All six values are disjoint and frozen; a new hashing
context takes a fresh tag.

| Tag | Value | Preimage |
|---|---|---|
| `LEAF_TAG` | `0x01` | message-MMR leaf |
| `INNER_TAG` | `0x02` | message-MMR inner node |
| `PEAK_TAG` | `0x03` | message-MMR peak bagging |
| `EMPTY_TAG` | `0x04` | empty-frontier root (§3.3) |
| `TREE_LEAF_TAG` | `0x05` | commitment-tree leaf (`STREAMS_LEAF_TAG` in code) |
| `TREE_INNER_TAG` | `0x06` | commitment-tree inner node (`STREAMS_INNER_TAG` in code) |

Values, not names, are consensus.

## 2. `StreamId` — canonical 8-byte encoding

Exactly 8 bytes, manual codec. `KEY_BITS = 64`.

```
Channel   : 0x00 ‖ recipient:u32be ‖ domain:u8  ‖ num:u16be
Ack       : 0x01 ‖ recipient:u32be ‖ domain:u8  ‖ num:u16be
Broadcast : 0x02 ‖ domain:u16be    ‖ subdomain:u8 ‖ num:u32be
Private   : kind:u8 (0x80..=0xFF) ‖ body:[u8;7]
```

Decode rules: fixed length 8; `decode ∘ encode = identity`; kinds `0x03..=0x7F` are rejected
(reserved). `Ord` on `StreamId` is the lexicographic order of these bytes, which equals the numeric
order of the field tuple.

Private kinds start at `0x80` so byte order agrees with variant order (`Channel < Ack < Broadcast <
Private`); the `StreamsRoot` trie splits on that order. Standard kinds grow up from `0x03`, private
down from `0xFF`.

## 3. Message MMR

### 3.1 Leaf

```
leaf = H(LEAF_TAG ‖ LEAF_VERSION ‖ payload)        LEAF_VERSION = 0x00
```

The preimage is transient. `LEAF_VERSION` versions this layout only; epochs are hash-disjoint. No
source, destination, position, or length prefix (design §Leaf Hashing).

Pinned: `H(0x01 ‖ 0x00 ‖ "hello")` =
`cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b`.

### 3.2 Inner node and peak bagging

```
inner = H(INNER_TAG ‖ left ‖ right)
root  = bag(peaks):  bag([p]) = p
        bag([p1..pn]) = H(PEAK_TAG ‖ bag([p2..pn]) ‖ p1)
```

Peaks are ordered highest (largest subtree, leftmost) to lowest. Bagging is a right fold with the
accumulated right side as the first hash argument (`mmr_lib`'s `merge_peaks(right, left)`).

### 3.3 Frontier and position

```rust
struct MmrFrontier { leaf_count: u64, peaks: Vec<Hash> }  // peaks high→low, ≤ 64
struct MessagePosition(u64);                              // leaf index, 0-based
```

The peak set is a pure function of `leaf_count`; `mmr_size = leaf_index_to_mmr_size(leaf_count −
1)`. Positions are derived (`frontier.leaf_count + i`), never stored. `leaf_count ≤
MAX_MMR_LEAF_COUNT = 2^48`; `MmrFrontier::from_parts` rejects a larger count or a peak count that
does not match the count's set bits.

The empty frontier has the defined root

```
empty_root = H(EMPTY_TAG) =
  642206314f534b29ad297d82440a5f9f210e30ca5ced805a587ca402de927342
```

It compares like any other root: the `Interval.start` of a stream's first consumption is this
constant (§10), and the identity extension on an empty frontier yields it.

## 4. Stream commitment tree (`StreamsRoot`)

Binary compact (Patricia) trie keyed by the canonical `StreamId` bytes.

### 4.1 Node hashing

```
leaf  = H(TREE_LEAF_TAG  ‖ StreamId:8 ‖ MmrRoot:32)          (41-byte preimage)
inner = H(TREE_INNER_TAG ‖ split_bit:u8 ‖ left:32 ‖ right:32) (66-byte preimage)
```

`split_bit` ∈ `0..KEY_BITS`, counted from the key's most significant bit. Every one of the 64 key
bits is committed exactly once: as some branch's split bit or inside the leaf preimage (design
§Stream Commitment Tree, constraints 3–4).

### 4.2 Canonical construction

```
tree_hash(entries)         // entries non-empty, sorted by key, distinct
  [(k, r)]  => H(TREE_LEAF_TAG ‖ k ‖ r)
  otherwise => b = lowest bit offset at which keys differ
               (zeros, ones) = split at first key with bit b set
               H(TREE_INNER_TAG ‖ b ‖ tree_hash(zeros) ‖ tree_hash(ones))
```

Any node cache is rebuildable, never authoritative.

### 4.3 Inclusion proof

```rust
struct TreeStep { split_bit: u8, sibling: Hash }
struct StreamProof { steps: BoundedVec<TreeStep, ConstU32<64>> }  // alias TreeInclusionProof
```

Steps run leaf to root with `split_bit` strictly decreasing; reject anything else at decode or
verify. Verification for key `K` and computed root `R`: `h = H(TREE_LEAF_TAG ‖ K ‖ R)`; per step,
direction = bit `K[split_bit]` (0 = left), `h = H(TREE_INNER_TAG ‖ split_bit ‖ left ‖ right)`; the
final `h` must equal the target `StreamsRoot`. Uniqueness rests on §4.1, not on the ordering rule.

`Vec<(u8, Hash)>` and `Vec<TreeStep>` SCALE-encode identically; the container must be bounded at 64
at decode.

## 5. MMR extension proof

```rust
struct MMRExtensionProof {
    leaf_count: u64,            // plain u64, §5.3
    connecting_nodes: Vec<Hash> // positions derived, §5.1
}
```

### 5.1 Connecting nodes: positions derived

Positions are not carried. An MMR's shape is a pure function of its leaf count, so `(old.leaf_count,
self.leaf_count)` fixes every connecting-node position; the derivation is mmr-lib's
`ancestry_proof_positions` (paritytech/merkle-mountain-range#11), and node order is mmr-lib's
prev-peaks-proof order. Out-of-range positions are unrepresentable. A positioned `Vec<(u64, Hash)>`
form is not conformant.

### 5.2 Identity and regression rules

`{ leaf_count: 0, connecting_nodes: [] }` is the identity extension: it yields the verifier's own
root unchanged (the caught-up case, and the head check for register reads). It is unambiguous
because a genuine extension to an empty MMR cannot exist. Otherwise `leaf_count > old.leaf_count`
strictly; an equal count is rejected (`NotForward`). The design's Appendix B writes `≥`; strict is
the implemented and specified form. Verification computes and returns the new root and fails if the
node count is not exactly right for the pair.

### 5.3 `leaf_count` encoding

Plain `u64` (8 bytes, SCALE little-endian), the same as `MmrFrontier.leaf_count`. Not `Compact`.

## 6. `MmrInclusionProof` (wire-only)

mmr-lib `MerkleProof` form (`mmr_size` + items). It never crosses the node–runtime boundary (design
§Messaging Inherent): it is carried in `EventResponse` and verified node-side; the runtime's only
discipline is recomputation. Normative for interoperability, not for the STF.

## 7. Relay-visible objects

### 7.1 Newtypes

`StreamsRoot(Hash)` and `MmrRoot(Hash)`: transparent 32-byte SCALE, distinct types.

### 7.2 UMP signals

Variant indices: `SelectCore = 0`, `ApprovedPeer = 1`, `Provides = 2` (`StreamsRoot`), `Requires =
3` (`RequiresSet`). `MAX_UMP_SIGNALS = 4` equals the variant count; a candidate carrying all four is
well-formed.

### 7.3 `RequiresSet`

```rust
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>);

pub const MAX_COMMITMENT_ENTRIES: u32 = 256;
```

Manual `Decode` rejects an empty set and non-strictly-increasing `ParaId`s; `decode ∘ encode =
identity`. Construction goes through `try_from_iter`, which sorts and rejects duplicates. The bound
is consensus: decode rejects larger sets.

### 7.4 Header digest

`DigestItem::Consensus(SPMS_ENGINE_ID, streams_root.encode())`, `SPMS_ENGINE_ID = *b"SPMS"`, at most
one per header. A reader accepts exactly one `SPMS` item whose payload is exactly 32 bytes; anything
else (a second item, a trailing byte, a short payload) means the header carries no `StreamsRoot`.
Readers must agree on validity, so the rule is part of the format. Freeze the id before anything
cross-chain ships (§13 #3).

## 8. Lift transport

```rust
struct RequiresLift {
    advances: Vec<MMRExtensionProof>,  // one per interval-chain gap, gap order
    extension: MMRExtensionProof,      // endpoint → current stream state
    tree_proof: StreamProof,           // stream root → StreamsRoot
}
struct LiftsBySource(BoundedVec<(ParaId, Vec<RequiresLift>), ConstU32<MAX_COMMITMENT_ENTRIES>>);
```

`LiftsBySource` decode rejects non-strictly-increasing `ParaId`s and more than
`MAX_COMMITMENT_ENTRIES` sources. A candidate that requires nothing emits no `Requires` signal.
Within a source, lifts match the consumption record's streams positionally in canonical `StreamId`
order; a mispaired lift cannot verify because the tree walk binds the record's key. Carried in
`ParachainBlockData::V3 { lifts: LiftsBySource, .. }`, never in the block body or commitments.

## 9. Wire protocol (interoperability-normative)

One request-response protocol, `/spec-msg/exchange`, carries both request kinds in an envelope.
Variant indices are frozen:

```rust
enum ExchangeRequest  { Messages(MessagesRequest)  = 0, Event(EventRequest)  = 1 }
enum ExchangeResponse { Messages(MessagesResponse) = 0, Event(EventResponse) = 1 }
```

A response's variant must match its request's. Objects, SCALE, with the §5 proof encodings:

```
MessagesRequest  { stream, start, under, max_bytes }
MessagesResponse { base, leaf_version, payloads, start_peaks, extension, tree_proof }
EventRequest     { stream, under, at: Option<MessagePosition> }
EventResponse    { payload, leaf_version, inclusion, tree_proof }
```

Every response verifies against the requester-named `under`; `max_bytes = 0` requests lift
material only. The versioned libp2p protocol string is pinned by the node crate (§13 #6).

## 10. PVF synthesis semantics (consensus in `validate_block`)

```rust
struct Interval { start: MmrRoot, end: MmrFrontier }
```

- `start` of a stream's first consumption is the empty root `H(EMPTY_TAG)` (§3.3).
- `ConsumeItem::Channel { payloads }`: `start` = root of the stored inbound frontier, `end` = that
  frontier after appending the payloads.
- `ConsumeItem::Events { base, start_peaks, payloads }`: the frontier is rebuilt with
  `MmrFrontier::from_parts(start_peaks, base)`; `start` = its root, `end` = it after appending the
  payloads (`payloads.len() ≥ 1`; a register head read is the single-payload case). `base` and
  `start_peaks` are unproven hints; a lie yields a root no lift can bind. Replay guard: `base` must
  exceed the stream's highwater, which then becomes `base + len − 1`.
- One interval per stream per block; at most one inherent item per stream; strict-on-import, one
  invalid item invalidates the block. The inherent carries no roots and no proofs.
- `stitch`: intervals in bundle order; `next.start` equals the current root (the empty root for an
  empty frontier) or is bridged by exactly the next `advances` proof, forward only. Stray or missing
  advances invalidate.
- `build_requires_entry`: per source, lifts match the record's streams positionally and in equal
  number (`LiftCountMismatch`); every stream's lifted root must converge to one `StreamsRoot`
  (`DivergentRoots`); sources and lifts must match exactly, both directions.

Reference algorithms: design §Requires Lifting (`stitch`, `build_requires_entry`, `build_requires`).

## 11. Protocol constants

| Constant | Value | Status |
|---|---|---|
| `KEY_BITS` | 64 | frozen |
| `LEAF_VERSION` | 0x00 | current epoch |
| `MAX_MMR_LEAF_COUNT` | 2^48 | frozen, §3.3 |
| `MAX_COMMITMENT_ENTRIES` | 256 | frozen, §7.3 |
| `MAX_UMP_SIGNALS` | 4 | = variant count |
| `MAX_SPECULATIVE_MESSAGE_LEN` | 102 400 B | frozen; wire-enforced payload bound (`PayloadTooLarge`) |
| `MAX_EXTENSION_CONNECTING_NODES` / `MAX_INCLUSION_PROOF_ITEMS` | 256 / 128 | decode ceilings; must exceed the valid maxima, exact values not consensus |
| `SPMS_ENGINE_ID` | `*b"SPMS"` | implemented; freeze before cross-chain ships |
| `W` (RecentProvides ring) | 128 | relay-side, governance-adjustable |
| `MaxTouchedStreams` / `MaxContextGaps` | per-chain receiver constants | bound the receiver's inherent; `MaxTouchedStreams ≤ MAX_COMMITMENT_ENTRIES`, integrity-checked |
| `MaxMessagesPerBlock` / `MaxMsgLen` (≤ the wire bound) | per-chain sender constants | STF-enforced sender-side |
| Lift / advance PoV reservation | pallet weight constants | ⚠ OPEN, §13 #5 |

## 12. Conformance test vectors

A conforming implementation reproduces every family. Status of the Rust pins in the primitives
crate:

1. **StreamId**: encode/decode round-trips per kind, reserved-kind rejection, ordering. Pinned.
2. **MMR**: leaf known-answer (§3.1), empty root (§3.3), bagging at 5, 64 and 65 leaves (the last
   two from an independent implementation of §3.2), frontier round-trips. Pinned.
3. **Tree**: `tree_hash` known-answer, proof round-trip, non-decreasing step order rejected. Pinned.
   Adversarially-close keys and key-aliasing negatives: not pinned.
4. **Extension/advance**: the 3-vs-4-leaf equal-node-count case, identity (including the empty-root
   result on an empty frontier), regression, wrong node count. Pinned. Position derivation is
   cross-checked against `gen_ancestry_proof` to 256 leaves in mmr-lib itself.
5. **Relay objects**: `RequiresSet` / `LiftsBySource` canonical-decode acceptance and rejection, UMP
   signal indices. Pinned.
6. **End-to-end synthesis**: records + lifts → `RequiresSet`, covering the bare tree-proof path, a
   bundle gap bridged by an advance, multi-stream and multi-source convergence, divergent-root
   rejection. Pinned. Fixture generators are implementation-derived, not conformance evidence.
7. **Wire objects** (§9): envelope discriminants, request/response round-trips, `MmrInclusionProof`
   verification, header-digest extraction and canonical-digest rejection. Pinned.

Language-neutral vector files are not yet extracted (§13 #7). This document is the authority on the
bytes; a vector change is a spec change.

## 13. Open decisions

| # | Item | Status |
|---|---|---|
| 1 | ~~`connecting_nodes` encoding~~ | settled: `Vec<Hash>`, positions derived (§5.1) |
| 2 | ~~`StreamProof` decode bound~~ | settled: 64 (§4.3) |
| 3 | `SPMS_ENGINE_ID` freeze | implemented as `*b"SPMS"`; freeze before cross-chain ships (§7.4) |
| 4 | ~~`leaf_count` encoding~~ | settled: plain `u64` (§5.3) |
| 5 | Lift / advance PoV reservation constants | pallet weight constants; set with benchmarks, must sit above the design ceilings (~4.2 KB/stream, ~2.1 KB/gap) |
| 6 | ~~Event wire path~~ | settled: one envelope protocol (§9); the versioned protocol string is pinned by the node crate |
| 7 | Conformance vectors | extract language-neutral vector files; pin the tree adversarial negatives (§12) |
