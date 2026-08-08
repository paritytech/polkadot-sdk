# Speculative Messaging: Consensus-Critical Encoding Specification

Companion to [speculative-messaging-design.md](speculative-messaging-design.md) v0.5:
this document pins every byte that must be **bit-identical across all
implementations forever** — the surface the design doc defers with
"specified with the primitives". Unmarked sections are normative and
implemented in the primitives crate (`cumulus-primitives-spec-messaging`
/ `polkadot-primitives::v9`); items marked **⚠ DECISION** are still
open, and §13 lists them.

Conventions: `H(x)` = **BLAKE2b-256** (`SpecHasher`); `Hash` = 32 bytes;
`‖` = byte concatenation; SCALE unless stated otherwise; integer fields
inside the 8-byte `StreamId` are **big-endian** (deliberate deviation from
SCALE's little-endian — the encoding must sort like the field tuple).

---

## 1. Hash domain tags

One byte, always the first preimage byte. All six values are disjoint and
**frozen**; any new hashing context must take a fresh tag.

| Tag | Value | Preimage it opens |
|---|---|---|
| `LEAF_TAG` | `0x01` | message-MMR leaf |
| `INNER_TAG` | `0x02` | message-MMR inner node |
| `PEAK_TAG` | `0x03` | message-MMR peak bagging |
| `EMPTY_TAG` | `0x04` | the empty-frontier root (§3.3) |
| `TREE_LEAF_TAG` | `0x05` | commitment-tree leaf |
| `TREE_INNER_TAG` | `0x06` | commitment-tree inner node |

(Implementation constant names may differ — e.g. `STREAMS_LEAF_TAG` /
`STREAMS_INNER_TAG` for the tree tags; values, not names, are consensus.)

## 2. `StreamId` — canonical 8-byte encoding

Exactly 8 bytes, manual codec, no derive. `KEY_BITS = 64`.

```
Channel   : 0x00 ‖ recipient:u32be ‖ domain:u8  ‖ num:u16be
Ack       : 0x01 ‖ recipient:u32be ‖ domain:u8  ‖ num:u16be
Broadcast : 0x02 ‖ domain:u16be    ‖ subdomain:u8 ‖ num:u32be
Private   : kind:u8 (0x80..=0xFF) ‖ body:[u8;7]
```

Decode rules (consensus): fixed length 8; `decode ∘ encode = identity`;
kinds `0x03..=0x7F` are **rejected** (reserved; no consensus path decodes
an unknown kind). `Ord` on `StreamId` = lexicographic order of these bytes
(= numeric order of the field tuple, by construction).

## 3. Message MMR

### 3.1 Leaf

```
leaf = H(LEAF_TAG ‖ LEAF_VERSION ‖ payload)        LEAF_VERSION = 0x00
```

The preimage is transient (never stored, never sent). `LEAF_VERSION`
versions this preimage layout only; epochs are hash-disjoint. Deliberately
absent: source, destination, position, length prefix (design §Leaf
Hashing).

Pinned vector: `H(0x01 ‖ 0x00 ‖ "hello")` =
`cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b`.

### 3.2 Inner node and peak bagging

```
inner = H(INNER_TAG ‖ left ‖ right)
root  = bag(peaks):  bag([p]) = p
        bag([p1..pn]) = H(PEAK_TAG ‖ bag([p2..pn]) ‖ p1)
```

Peaks ordered highest (largest subtree, leftmost) to lowest; bagging is a
right fold in which the accumulated right side is the **first** hash
argument (`mmr_lib`'s `merge_peaks(right, left)` convention — the
preimage order is the reverse of the visual left-to-right).

### 3.3 Frontier and position

```rust
struct MmrFrontier { leaf_count: u64, peaks: Vec<Hash> }  // peaks high→low
struct MessagePosition(u64);                              // leaf index, 0-based
```

Which peaks exist is a pure function of `leaf_count` (binary
representation); `mmr_size = leaf_index_to_mmr_size(leaf_count − 1)`.
Positions are always derived (`frontier.leaf_count + i`), never stored.

The **empty frontier** (no peaks) has the defined root

```
empty_root = H(EMPTY_TAG) =
  642206314f534b29ad297d82440a5f9f210e30ca5ced805a587ca402de927342
```

— a comparable value like any other root: the `Interval.start` of a
stream's first-ever consumption is exactly this constant (§10), and the
identity extension applied to an empty frontier yields it.

## 4. Stream commitment tree (`StreamsRoot`)

Binary compact (Patricia) trie keyed by the canonical `StreamId` bytes.

### 4.1 Node hashing

```
leaf  = H(TREE_LEAF_TAG  ‖ StreamId:8 ‖ MmrRoot:32)          (41-byte preimage)
inner = H(TREE_INNER_TAG ‖ split_bit:u8 ‖ left:32 ‖ right:32) (66-byte preimage)
```

`split_bit` ∈ `0..KEY_BITS`, counted from the key's first (most
significant) bit. Both constraints are load-bearing (design §Stream
Commitment Tree, constraints 3–4): the split bit in the inner preimage,
the **full key** in the leaf preimage. Audit rule: every one of the 64 key
bits is committed exactly once — as some branch's split bit or inside the
leaf preimage.

### 4.2 Canonical construction

```
tree_hash(entries)         // entries non-empty, sorted by key, distinct
  [(k, r)]  => H(TREE_LEAF_TAG ‖ k ‖ r)
  otherwise => b = lowest bit offset at which keys differ
               (zeros, ones) = split at first key with bit b set
               H(TREE_INNER_TAG ‖ b ‖ tree_hash(zeros) ‖ tree_hash(ones))
```

Every implementation must reproduce this bit-identically; the stored node
cache is rebuildable, never authoritative.

### 4.3 Inclusion proof

```rust
struct TreeStep { split_bit: u8, sibling: Hash }
struct TreeInclusionProof { steps: BoundedVec<TreeStep, ConstU32<64>> }
```

Steps ordered **leaf to root**, `split_bit` **strictly decreasing**
(reject anything else at decode/verify — early garbage; uniqueness rests
on §4.1, not on this rule). Verification, for key `K` and computed root
`R`: `h = H(TREE_LEAF_TAG ‖ K ‖ R)`; per step, direction = bit `K[split_bit]`
(0 = we are left), `h = H(TREE_INNER_TAG ‖ split_bit ‖ left ‖ right)`;
final `h` must equal the target `StreamsRoot`.

Encoding note: `Vec<(u8, Hash)>` and `Vec<TreeStep>` SCALE-encode
identically; the **container must be bounded at 64 at decode** (an
unbounded `Vec` is a decode-DoS surface).

## 5. MMR extension proof

```rust
struct MMRExtensionProof {
    leaf_count: u64,            // u64 vs Compact<u64>, §5.3
    connecting_nodes: Vec<Hash> // 32 B nodes, positions derived — §5.1
}
```

### 5.1 Connecting nodes: `Vec<Hash>`, positions derived

Positions are *not* carried: an MMR's shape is a pure function of its leaf
count, so `(old.leaf_count, self.leaf_count)` fixes every connecting-node
placement deterministically (derivation = `ancestry_positions`,
cross-checked against `gen_ancestry_proof`; node order = `mmr_lib`'s
prev-peaks-proof order). 32 B/node, and out-of-range positions are
unrepresentable rather than checked (reviewed on #12659 thread
r3664785341). A `Vec<(u64, Hash)>` positioned form is **not** conformant
(§13 #1).

### 5.2 Identity and regression rules

`{ leaf_count: 0, connecting_nodes: [] }` is the **identity extension**:
yields the verifier's own root unchanged (the caught-up case — also the
head-ness check for register reads). Unambiguous: a genuine extension to
an empty MMR cannot exist. Otherwise require
`leaf_count > old.leaf_count` — **strictly** forward: an equal-count
"extension" is the identity in non-canonical clothing and is rejected
(`NotForward`; note the design's Appendix B writes `≥`, which would admit
that second encoding — strictness is the implemented and specified form).
Verification **computes and returns** the new root (never declared
alongside), failing if the node count is not exactly right for the pair.

### 5.3 `leaf_count` encoding — **⚠ DECISION**

The design doc writes `Compact<u64>`; the implementation uses plain
`u64` (fixed 8 bytes). Either is sound; they are wire-incompatible. The
cheap resolution is `u64` + a one-word design-doc fix; `Compact` saves
~4 B/proof if preferred.

## 6. `MmrInclusionProof` (wire-only)

`mmr_lib` `MerkleProof` form (`mmr_size` + sibling/peak items). **Never
crosses the node–runtime boundary** (design §Messaging Inherent) — it is a
wire object (`EventResponse`), verified node-side; the runtime's only
discipline is recomputation. Normative for interoperability, not for the
STF.

## 7. Relay-visible objects

### 7.1 Newtypes

`StreamsRoot(Hash)` and `MmrRoot(Hash)` — transparent 32-byte SCALE,
distinct types (must not be confusable in code; bytes are identical).

### 7.2 UMP signals

Variant indices (consensus): `SelectCore = 0`, `ApprovedPeer = 1`,
`Provides = 2` (payload: `StreamsRoot`), `Requires = 3` (payload:
`RequiresSet`). `MAX_UMP_SIGNALS = 4` — must equal the variant count (a
candidate carrying all four distinct signals is well-formed).

### 7.3 `RequiresSet`

```rust
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), ConstU32<MAX_COMMITMENT_ENTRIES>>);

pub const MAX_COMMITMENT_ENTRIES: u32 = 256;
```

Manual `Decode` **rejects** non-strictly-increasing `ParaId`s (no silent
normalization; `decode ∘ encode = identity`). Construction sealed
(`try_from_iter` sorts + rejects duplicates). The bound is 256
(registered-para count ~200, rounded up, one name everywhere) and is
consensus-relevant: decode rejects larger sets, so all implementations
must agree on it.

### 7.4 Header digest — **⚠ DECISION: engine id value**

`DigestItem::Consensus(SPMS_ENGINE_ID, streams_root.0.encode())`, at most
one per header; foreign nodes parse it directly (protocol standard, not
chain-internal). Proposed value: **`*b"SPMS"`** (self-describing), to be
frozen before anything cross-chain ships.

## 8. Lift transport

```rust
struct RequiresLift {
    advances: Vec<MMRExtensionProof>,  // one per interval-chain gap, gap order
    extension: MMRExtensionProof,      // endpoint → current stream state
    tree_proof: TreeInclusionProof,    // stream root → StreamsRoot
}
// transported grouped per source:
lifts: Vec<(ParaId, Vec<RequiresLift>)>
```

Decode rejects non-strictly-increasing `ParaId`s (same canonicality
discipline as `RequiresSet`).
Within a source, lifts match the consumption record's streams
**positionally** in canonical `StreamId` order — a mispaired lift cannot
verify (the tree walk binds the record's key). Carried in
`ParachainBlockData` as a new versioned variant (per the `V2 →
scheduling_proof` precedent); never in the block body or commitments.

## 9. Wire protocol (interoperability-normative)

Request-response protocol name: **`/spec-msg/exchange/1`** (the design's
`EventRequest`/`EventResponse` pair rides the same protocol —
⚠ confirm single- vs two-protocol split when the event path lands).

Both request kinds travel in one **exchange envelope**; the discriminant
byte is part of the wire format and the variant indices are **frozen**:

```rust
enum ExchangeRequest  { Messages(MessagesRequest)  = 0, Event(EventRequest)  = 1 }
enum ExchangeResponse { Messages(MessagesResponse) = 0, Event(EventResponse) = 1 }
```

A response's variant must match its request's — a `Messages` request
answered by an `Event` response is malformed, independently of its
proofs.

Objects exactly as the design doc's §Fetch Protocol:
`MessagesRequest { stream, start, under, max_bytes }`,
`MessagesResponse { base, leaf_version, payloads, start_peaks, extension,
tree_proof }`, `EventRequest { stream, under, at }`,
`EventResponse { payload, inclusion, tree_proof }` — SCALE, with the §5
proof encodings. Every response verifies against the requester-named
`under`; `max_bytes = 0` = payload-free (pure lift material).

## 10. PVF synthesis semantics (consensus in `validate_block`)

```rust
struct Interval { start: MmrRoot, end: MmrFrontier }
```

- `start` of a stream's **first-ever** consumption = the empty root
  `H(EMPTY_TAG)` (§3.3).
- Interval formation: channels — `start`/`end` = the stored frontier's
  root before / the frontier after the block's messages. Reads
  (`Events`) — a self-loop: `end` = the verified read context, `start` =
  `bag(end.peaks)`; context jumps surface as `stitch` gaps, never inside
  an interval.
- One interval per stream per block (the inherent enforces at most one
  item per stream; strict-on-import — one invalid item invalidates the
  block).
- `stitch`: intervals in bundle order; `next.start` must equal
  `current`'s root (§3.3 — the empty root for an empty frontier) or be
  bridged by exactly the next `advances` proof (forward-only); stray or
  missing advances invalidate.
- `build_requires_entry`: per source, lifts match the record's streams
  positionally and in **equal number** (`LiftCountMismatch`); every
  stream's lifted root must converge to **one** `StreamsRoot`
  (`DivergentRoots`); sources must match lifts exactly, both directions.

Reference algorithms: design §Requires Lifting (`stitch`,
`build_requires_entry`, `build_requires` pseudocode is normative).

## 11. Protocol constants

| Constant | Value | Status |
|---|---|---|
| `KEY_BITS` | 64 | frozen |
| `LEAF_VERSION` | 0x00 | current epoch |
| `W` (RecentProvides ring) | 128 | governance-adjustable |
| `MAX_COMMITMENT_ENTRIES` | 256 | frozen — §7.3 |
| `MAX_UMP_SIGNALS` | 4 | = variant count |
| `MAX_SPECULATIVE_MESSAGE_LEN` | 102 400 B | frozen — wire-enforced hard payload bound (`PayloadTooLarge`) |
| Lift reservation | design ceiling ~4.2 KB/stream; implemented `LIFT_RESERVATION_BYTES` = 4096 | **⚠ align** — the constant sits below the design's stated ceiling |
| Advance reservation | design ceiling ~2.1 KB/gap; implemented `ADVANCE_PROOF_RESERVATION_BYTES` = 2048 | **⚠ align** — same |
| `MAX_EXTENSION_CONNECTING_NODES` / `MAX_INCLUSION_PROOF_ITEMS` | 256 / 128 | defense ceilings — must exceed the valid maxima; exact values not consensus |
| `MaxTouchedStreams` / `MaxContextGaps` | per-chain receiver constants; reference values 32 / 8 | bound the receiver's own inherent + PoV reservation — nothing cross-chain observes them; constraint `MaxTouchedStreams` ≤ 256 (integrity-checked) |
| `MaxMessagesPerBlock` / per-chain `MaxMsgLen` (≤ the wire bound) | sender-chain constants | per-chain, STF-enforced sender-side |
| `SPMS_ENGINE_ID` | **⚠ `*b"SPMS"` proposed**, to be frozen | §7.4 |

## 12. Conformance test vectors (normative requirement)

The design doc mandates vectors for `StreamId` only; this spec extends
the mandate to the full surface — a conforming implementation must
reproduce every family:

1. **StreamId**: encode/decode round-trips per kind, reserved-kind
   rejection, ordering (exists).
2. **MMR**: leaf known-answer (§3.1 — pinned in the PoC; port to this
   crate), empty root (§3.3 ✓ exists), append/bagging sequences to ≥ 64
   leaves (5-leaf pin exists), frontier round-trips.
3. **Tree**: `tree_hash` known-answers (1, 2, 4, adversarially-close
   keys), proof verification incl. **negative vectors** (non-decreasing
   step order, wrong split bit, key-aliasing attempts per §4.1
   constraint 4).
4. **Extension/advance**: known-answer proofs across leaf-count pairs
   (incl. the 3-vs-4-leaf same-node-count ambiguity case), identity,
   regression rejection, wrong-node-count rejection.
5. **Relay objects**: `RequiresSet`/lift-transport canonical-decode
   acceptance + rejection vectors; UMP signal indices.
6. **End-to-end synthesis**: consumption records + lifts → `RequiresSet`,
   covering hot path (bare tree proof), gap + advance, divergent-root
   rejection, first-consumption empty-root start. (Shared fixture
   machinery — the PoC's `test_utils` stream/lift generators — produces
   this material for tests, but is implementation-derived, not itself
   conformance evidence.)
7. **Wire objects** (§9): exchange-envelope discriminants (both
   directions), request/response SCALE round-trips, `MmrInclusionProof`
   verification vectors, header-digest extraction. No pins exist today.

Vectors are **language-neutral files** shipped with the primitives crate;
the in-code pinned tests are their Rust binding (today: pinned tests
exist partially, vector files not yet). **This document is the authority
on the bytes** — a vector change is a spec change.

## 13. Open decisions

| # | Item | Proposal |
|---|---|---|
| 1 | `connecting_nodes` encoding in the PoC | migrate to `Vec<Hash>`, derived positions (§5.1) |
| 2 | `TreeInclusionProof` decode bound in the PoC | bound at 64 (§4.3) |
| 3 | `SPMS_ENGINE_ID` freeze | `*b"SPMS"` (§7.4) |
| 4 | `leaf_count` encoding | `u64` (as implemented) + one-word design-doc fix, or `Compact` (§5.3) |
| 5 | Reservation constants | bump `LIFT_RESERVATION_BYTES` / `ADVANCE_PROOF_RESERVATION_BYTES` above the design ceilings (§11 ⚠ align) |
| 6 | Event wire path | single- vs two-protocol split (§9) |
| 7 | Conformance vectors | port the leaf pin into the primitives; add wire-object pins; extract language-neutral vector files (§12) |

Everything else in this document is settled: the byte-level rules are
implemented in the primitives crate; the surfaces beyond it (inherent
dispatch, the relay ring, archive serving) live in the pallet / relay /
client implementations.
