# Speculative Messaging: High-Level Overview

## Problem Statement

HRMP (Horizontal Relay-routed Message Passing) routes all cross-chain messages through the relay chain state. This imposes a minimum latency of 2-3 relay chain blocks (12-18+ seconds) because:

1. ParaA produces a block containing outgoing messages
2. ParaA's block is included on the relay chain (relay block N)
3. The relay chain processes the HRMP channel queue
4. ParaB sees the messages in relay state (relay block N+1 at earliest)
5. ParaB includes the messages in its next block

Speculative Messaging eliminates this bottleneck by moving message content **off-chain** and verifying only **cryptographic commitments** on-chain.

## Architecture Summary

```
                         RELAY CHAIN
                 +-----------------------+
                 | IncludedProvidesRoots  |
                 |   ParaA -> root_A     |
                 |   ParaB -> root_B     |
                 |                       |
                 | Inclusion Check:      |
                 |  requires.root ==     |
                 |  provides.root        |
                 +-----------+-----------+
                      ^            ^
                      |            |
           provides   |            |  provides
           requires   |            |  requires
                      |            |
              +-------+--+    +---+--------+
              |  ParaA   |    |   ParaB    |
              |  Runtime |    |   Runtime  |
              +----+-----+    +-----+------+
                   |                |
        send_msg   |                |  receive_msg
                   |                |
              +----+-----+    +-----+------+
              | CollatorA|    | CollatorB  |
              +----+-----+    +-----+------+
                   |                ^
                   |  off-chain     |
                   | MessageBatch   |
                   +----> relay --->+
                         peers
```

## Core Concepts

### Provides Commitment

When a parachain sends messages, it maintains a **per-destination MMR** (Merkle Mountain Range) for each destination chain. Each message hash is appended as a leaf. The roots of all per-destination MMRs are organized into a **top-level binary Merkle tree** keyed by `ParaId`. The root of this top-level tree is the **ProvidesCommitment** — a single `H256` hash published on-chain.

```
ProvidesCommitment (top-level root)
        |
   Merkle Tree
   /    |    \
ParaB  ParaC  ParaD    <- per-destination MMR roots
 |      |      |
MMR    MMR    MMR       <- each is a Merkle Mountain Range
/|\    /|\    /|\
msg    msg    msg       <- individual message hashes as leaves
```

### Requires Commitment

When a parachain receives messages from a source, it emits a **RequiresCommitment** containing:
- `source: ParaId` — which chain the messages came from
- `expected_root: H256` — the provides root of the source chain that was used to verify the messages

The relay chain matches `requires.expected_root == provides.root` for the given source.

### Late Block Proofs

Timing mismatches occur when a receiver built its block against an **older** provides root than what the source has since published. A **LateBlockProof** (carried in the PoV) proves that:

1. The old subtree root was in the old provides tree
2. The new subtree root is in the current provides tree
3. The old MMR is a prefix of the new MMR (via an MMR extension proof)

The PVF transforms the requires commitment from referencing the old root to the current root.

### Domain Prefixes (Hash Collision Prevention)

All hashing uses blake2_256 with domain separation:

| Prefix | Usage |
|--------|-------|
| `0x00` | Merkle tree leaf hashes: `hash_leaf(ParaId, mmr_root)` |
| `0x01` | Merkle tree internal nodes: `hash_pair(left, right)` |
| `0x02` | MMR node merges: `merge_mmr_nodes(left, right)` |
| `0x03` | Message leaf hashes: `OutgoingMessage::leaf_hash()` |

## End-to-End Message Flow

### Phase 1: Sending (ParaA)

1. **Application calls `send_message(dest=ParaB, payload)`**
2. Pallet computes `leaf_hash = blake2_256(0x03 ++ encode(OutgoingMessage))`
3. Leaf pushed into `DestinationMmrs[ParaB]` (per-destination MMR)
4. `TopLevelTree` updated with new MMR root for ParaB (O(log D) rehash)
5. Message stored in `PendingOutgoing[ParaB]` for collator networking
6. `on_finalize` reads `TopLevelTree.root()` as the `ProvidesCommitment`

### Phase 2: Off-Chain Distribution

7. Collator observes ParaA's **best block** (not finalized — speculative!)
8. Reads `PendingOutgoing` storage from ParaA's state
9. Constructs `MessageBatch` with Merkle inclusion proof
10. Sends `ForwardMessageRequest` to ParaB's relay peer via `/polkadot/spec-msg/1` protocol
11. ParaB's collator receives, verifies subtree inclusion proof, queues batch

### Phase 3: Receiving (ParaB)

12. ParaB's collator drains incoming batches during block authoring
13. Creates `receive_messages_inherent` with `(source=ParaA, count, provides_root)` tuples
14. Pallet advances `PerSourceState[ParaA]` and records `RequiresCommitment`
15. `on_finalize` reads `PendingRequires` as the list of `RequiresCommitment`s

### Phase 4: PVF Validation

16. `validate_block` executes the block and collects provides/requires from pallet
17. If late block proofs exist in PoV, verifies each one:
    - Old subtree proof against old provides root
    - New subtree proof against current provides root
    - MMR extension proof (if subtree grew)
18. Transforms requires commitments to reference current provides roots
19. Returns `ValidationResult` with `provides_spec_msg_root` and `requires_spec_msg`

### Phase 5: Relay Chain Verification

20. Relay chain receives the candidate with `CandidateCommitments`
21. For each `provides`: stores root in `IncludedProvidesRoots[ParaA]`
22. For each `requires`: verifies `expected_root == IncludedProvidesRoots[source].root`
23. If all match, candidate is accepted; otherwise rejected with `SpeculativeMessagingMismatch`

## Latency Comparison

| Mechanism | Latency | Why |
|-----------|---------|-----|
| HRMP | 2-3 relay blocks (12-18s) | Messages routed through relay state |
| Speculative Messaging | ~1 relay block (6s) | Messages exchanged off-chain at best-block time |

## Component Map

| Component | Location | Purpose |
|-----------|----------|---------|
| [Primitives](./01-primitives.md) | `polkadot/primitives/speculative-messaging/` | Types, proofs, traits |
| [Merkle Tree](./02-merkle-tree.md) | (within primitives) | Binary Merkle tree + StoredMerkleTree |
| [Pallet](./03-pallet.md) | `cumulus/pallets/speculative-messaging/` | Runtime send/receive logic |
| [Relay Inclusion](./04-relay-inclusion.md) | `polkadot/runtime/parachains/src/inclusion/` | On-chain commitment matching |
| [PVF Late Block](./05-pvf-late-block.md) | `cumulus/pallets/parachain-system/src/validate_block/` | Late block proof verification |
| [Offchain Networking](./06-offchain-networking.md) | `cumulus/client/speculative-messaging/` | Collator message exchange |
| [Parachain System](./07-parachain-system-integration.md) | `cumulus/pallets/parachain-system/` | Glue: pallet to commitments |

## Commit History

| Commit | Component |
|--------|-----------|
| `bed687297c` | Primitives crate (types, proofs, MMR, Merkle tree) |
| `7cbd7713a2` | StoredMerkleTree for incremental pallet builder |
| `74d7a5cf35` | Security fixes (domain prefixes, source validation, bounds) |
| `adc7e3fa1f` | More fixes (duplicate peaks, u32 overflow, validate()) |
| `d6e4bbb9e8` | Pallet for speculative messaging |
| `87135c7807` | Pallet fixups and more tests |
| `a112f1054a` | Relay side inclusion and MMR root verification |
| `e27b6f9320` | Make requirements bounded (DoS prevention) |
| `2187baba12` | PVF late block validation |
| `8cbdf69c5e` | Offchain message exchange with priority peer lists |
| `eb718ec698` | Parachain-system on_finalize integration |
| `a0c5fca931` | Inherent support, test runtime, zombienet E2E test |
| `37501b4184` | Speculate on best block instead of finalized |

## What Is Excluded from MVP

- **SuperChain Collators** — no intra-block messaging, no SuperBlock, no co-authoring
- **Acknowledgement-based speculation** — MVP uses inclusion-based trust only
- **Trust domain configuration** — all chains treated as cross-domain
- **Cycle prevention** — not needed without intra-block messaging
- **HRMP deprecation** — HRMP stays as parallel fallback
