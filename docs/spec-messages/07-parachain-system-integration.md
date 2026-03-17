# Speculative Messaging: Parachain System Integration

**Location:** `cumulus/pallets/parachain-system/src/lib.rs`

This document covers the glue between the speculative messaging pallet and the relay chain validation pipeline. It also covers the test runtime configuration and the zombienet E2E test.

## Config Trait Extension

```rust
pub trait Config: frame_system::Config {
    // ... existing fields ...

    /// Provider of speculative messaging commitments.
    /// Called during `on_finalize` to collect provides/requires for ValidationResult.
    type SpeculativeMessagingProvider:
        polkadot_primitives_speculative_messaging::SpeculativeMessagingProvider;
}
```

For runtimes without speculative messaging: `type SpeculativeMessagingProvider = ();`

---

## on_finalize Integration

During `on_finalize`, after HRMP message processing:

```rust
// Collect speculative messaging commitments
let provides_root = T::SpeculativeMessagingProvider::provides_root();
let requires = T::SpeculativeMessagingProvider::requires_commitments();

// Store in ephemeral storage for validate_block to read
if let Some(root) = provides_root {
    ProvidesSpecMsgRoot::<T>::put(root);
}
if !requires.is_empty() {
    PendingRequiresSpecMsg::<T>::put(
        BoundedVec::try_from(requires)
            .expect("bounded by MAX_REQUIRES_COMMITMENT_NUM; qed")
    );
}
```

### Ephemeral Storage Items

```rust
/// Speculative messaging provides root (cleared each block)
#[pallet::storage]
pub type ProvidesSpecMsgRoot<T> = StorageValue<_, H256>;

/// Speculative messaging requires commitments (cleared each block)
/// Bounded by MAX_REQUIRES_COMMITMENT_NUM
#[pallet::storage]
pub type PendingRequiresSpecMsg<T> = StorageValue<_,
    BoundedVec<RequiresCommitment, ConstU32<MAX_REQUIRES_COMMITMENT_NUM>>
>;
```

Both are cleared in `on_initialize` of the next block.

---

## Flow: Pallet to ValidationResult

```
Block Execution
================
1. send_message() calls -> update DestinationMmrs, TopLevelTree
2. receive_messages_inherent() -> update PerSourceState, PendingRequires

on_finalize
================
3. pallet-parachain-system calls:
   T::SpeculativeMessagingProvider::provides_root()
     -> reads TopLevelTree.root() from speculative-messaging pallet
   T::SpeculativeMessagingProvider::requires_commitments()
     -> reads PendingRequires from speculative-messaging pallet

4. Stores into ephemeral storage:
   ProvidesSpecMsgRoot = Some(H256)
   PendingRequiresSpecMsg = Vec<RequiresCommitment>

validate_block (PVF)
================
5. Reads ephemeral storage after block execution
6. Processes late block proofs (if any)
7. Returns ValidationResult:
   - provides_spec_msg_root: Option<H256>
   - requires_spec_msg: BoundedVec<(Id, H256), MAX>

Relay Chain
================
8. Converts ValidationResult to CandidateCommitments:
   - provides: Option<ProvidesCommitment>
   - requires: BoundedVec<RequiresCommitment, MAX>
9. Stores provides in IncludedProvidesRoots
10. Validates requires against IncludedProvidesRoots
```

---

## Test Runtime Configuration

**File:** `cumulus/test/runtime/src/lib.rs`

### Feature-Gated WASM Binary

```rust
pub mod speculative_messaging {
    #[cfg(feature = "std")]
    include!(concat!(env!("OUT_DIR"), "/wasm_binary_speculative_messaging.rs"));
}
```

### Pallet Configuration

```rust
impl cumulus_pallet_speculative_messaging::Config for Runtime {
    type MaxDestinations = ConstU32<100>;
    type MaxSources = ConstU32<100>;
    type MaxMessagesPerBlock = ConstU32<1000>;
    type MaxPayloadSize = ConstU32<1024>;
}
```

### Parachain System Configuration

```rust
impl cumulus_pallet_parachain_system::Config for Runtime {
    #[cfg(feature = "speculative-messaging")]
    type SpeculativeMessagingProvider = cumulus_pallet_speculative_messaging::Pallet<Runtime>;
    #[cfg(not(feature = "speculative-messaging"))]
    type SpeculativeMessagingProvider = ();
}
```

### construct_runtime!

```rust
construct_runtime! {
    // ... existing pallets ...
    SpeculativeMessaging: cumulus_pallet_speculative_messaging,
}
```

---

## Zombienet E2E Test

**File:** `cumulus/zombienet/zombienet-sdk/tests/zombie_ci/speculative_messaging.rs`

### Network Topology

```
Relay Chain (Rococo)
  Validator Alice
  Validator Bob

ParaA (2000)                    ParaB (2001)
  CollatorA (Alice)               CollatorB (Alice)
```

### Test Phases

#### Phase 1: Network Setup

1. Spawn relay chain with 2 validators
2. Register parachains 2000 and 2001
3. Wait for first session change (paras active)
4. Both parachains reach block height >= 2

#### Phase 2: Relay Peer Registration

```
1. Extract CollatorA's relay peer ID from logs:
   "Local node identity is: 12D3..."

2. Extract CollatorB's relay peer ID from logs

3. Submit sudo extrinsic on ParaA:
   SpeculativeMessaging::set_relay_peer(PARA_B=2001, CollatorB_peer_id)

4. Submit sudo extrinsic on ParaB:
   SpeculativeMessaging::set_relay_peer(PARA_A=2000, CollatorA_peer_id)
```

This enables bidirectional off-chain message routing.

#### Phase 3: Message Sending

```
1. Record T_send = now()
2. Record relay_block_before = current relay block height
3. Record para_b_block_before = current ParaB block height

4. Submit signed extrinsic on ParaA:
   SpeculativeMessaging::send_message_extrinsic(
       destination: PARA_B,
       payload: b"hello-spec-msg"
   )

5. Wait for ParaA finalization
6. Verify MessageSent event emitted
7. Record T_sent = now()
```

#### Phase 4: Message Receipt

```
1. Poll ParaB's finalized blocks
2. For each new block, check for MessagesReceived event
3. When found:
   - Record T_received = now()
   - Record para_b_block_received = block number
   - Extract event data: { source, count, provides_root }
   - Verify source == PARA_A
```

#### Phase 5: Relay Latency Measurement

```
1. Scan relay chain finalized blocks for CandidateIncluded events

2. Find relay block where ParaA's candidate was included
   (after the send — has the provides root)

3. Find relay block where ParaB's candidate was included
   (after the receive — has the requires commitment)

4. relay_latency = para_b_relay_block - para_a_relay_block
```

#### Phase 6: Assertions

```
delivery_latency = T_received - T_sent
assert!(delivery_latency < 30 seconds)

relay_latency = para_b_relay_block - para_a_relay_block
assert!(relay_latency <= 3 relay blocks)

// Both paras continue producing (health check)
assert!(para_a_block_now > para_a_block_before)
assert!(para_b_block_now > para_b_block_before)
```

### Expected Results

| Metric | Expected | Why |
|--------|----------|-----|
| Delivery latency | < 18s | Off-chain exchange at best-block time |
| Relay block latency | 1-2 blocks | Only need ParaA inclusion + ParaB inclusion |
| HRMP comparison | 2-3 blocks | Messages routed through relay state |

---

## "Speculate on Best Block" Change

**Commit:** `37501b4184`

**File:** `cumulus/test/service/src/lib.rs`

### Before (Finalized Blocks)

```rust
let mut finality_stream = outbound_client.finality_notification_stream();
```

Messages were only distributed after the source block was finalized on the relay chain.

### After (Best Blocks)

```rust
let mut import_stream = outbound_client.import_notification_stream();

while let Some(notification) = import_stream.next().await {
    if !notification.is_new_best {
        continue;  // Skip reorgs and non-best forks
    }
    // Read PendingOutgoing, construct batches, distribute
}
```

### Why This Matters

```
Timeline with finalized blocks:
  T0:  ParaA produces block
  T6:  ParaA's block finalized (1 relay block)
  T6:  Outbound distributor sends messages   <-- DELAYED
  T12: ParaB receives, builds block
  T18: ParaB's block included on relay
  Total: ~18 seconds (3 relay blocks)

Timeline with best blocks:
  T0:  ParaA produces block
  T0:  Outbound distributor sends messages   <-- IMMEDIATE
  T6:  ParaB receives, builds block
  T12: ParaB's block included on relay
  Total: ~12 seconds (2 relay blocks)
  Or even ~6 seconds if ParaB is fast
```

**Safety:** If ParaA's best block is reverted, the provides root won't match on the relay chain and ParaB's candidate will be rejected. The speculative approach trades occasional rejections for consistently lower latency.

---

## Diagram: End-to-End Integration

```
ParaA Runtime                         ParaB Runtime
=============                         =============

send_message(ParaB, payload)
  |
  v
DestinationMmrs[ParaB].push()
TopLevelTree.update(ParaB)
PendingOutgoing[ParaB].push(msg)
  |
  v
on_finalize:
  provides_root = TopLevelTree.root()
  ProvidesSpecMsgRoot = provides_root
         |
         v                            receive_messages_inherent(entries)
  validate_block:                       |
    provides_spec_msg_root = root       v
    requires_spec_msg = []            receive_messages(ParaA, count, root)
         |                              |
         v                              v
  CandidateCommitments:               PerSourceState[ParaA].advance()
    provides = ProvidesCommitment     PendingRequires.push(req)
    requires = []                       |
                                        v
                                      on_finalize:
                                        requires = PendingRequires
                                        PendingRequiresSpecMsg = requires
                                              |
                                              v
                                        validate_block:
                                          late_block_proofs? -> verify & transform
                                          requires_spec_msg = transformed
                                              |
                                              v
                                        CandidateCommitments:
                                          provides = ParaB's root
                                          requires = [{source: A, root: root_A}]

Relay Chain:
  1. Include ParaA candidate -> IncludedProvidesRoots[A] = root_A
  2. Include ParaB candidate -> check requires[0].root == IncludedProvidesRoots[A].root
     Match! -> Accept
```
