# Speculative Messaging: Offchain Networking

**Location:** `cumulus/client/speculative-messaging/`

This component handles the off-chain exchange of messages between collators via relay chain peers. It is the networking layer that makes speculative messaging possible — messages flow directly between chains without waiting for relay chain state.

## Architecture

```
ParaA Collator                                     ParaB Collator
+---------------+                                 +---------------+
| Block Author  |                                 | Block Author  |
|      |        |                                 |      ^        |
|      v        |                                 |      |        |
| PendingOut-   |    ForwardMessageRequest         | drain_        |
| going storage |----> Outbound -----> Relay ----> | incoming()    |
|               |    Distributor      Peers        |               |
+---------------+                                 +---------------+
                         |                    |
                   /polkadot/spec-msg/1  (request/response)
```

## Module Structure

```
src/
  lib.rs          — Module documentation, re-exports
  error.rs        — Error types
  protocol.rs     — Wire types (ForwardMessageRequest, ForwardMessageResponse)
  registry.rs     — PeerRegistry trait + HardcodedPeerRegistry impl
  service.rs      — SpeculativeMessagingWorker (main event loop)
```

---

## Wire Protocol

**Protocol name:** `/polkadot/spec-msg/1`

**Max request size:** 16 MiB (message batches can be large)

**Max response size:** 1 KB (lightweight status)

### ForwardMessageRequest

```rust
pub struct ForwardMessageRequest {
    pub source_para: ParaId,       // Source parachain that produced the messages
    pub destination_para: ParaId,   // Target parachain
    pub batch: MessageBatch,        // Messages + Merkle proof
}
```

### ForwardMessageResponse

```rust
pub enum ForwardMessageResponse {
    Accepted,                       // Batch accepted by destination collator
    Forwarded,                      // Batch forwarded to next relay peer hop
    Rejected { reason: Vec<u8> },  // Rejected with human-readable reason
}
```

### MessageBatch (from primitives)

```rust
pub struct MessageBatch {
    pub source: ParaId,
    pub source_block: H256,              // Hash of source block
    pub provides_root: H256,             // Source's top-level Merkle root
    pub subtree_root: H256,              // Per-destination MMR root for receiver
    pub subtree_inclusion_proof: MerkleProof,  // Proof subtree_root in provides_root
    pub messages: Vec<OutgoingMessage>,  // Actual messages, ordered by position
}
```

---

## Peer Registry

### Trait

```rust
pub trait PeerRegistry: Send + Sync {
    fn get_peer(&self, para_id: ParaId) -> Option<OpaquePeerId>;
    fn set_peer(&self, para_id: ParaId, peer_id: OpaquePeerId);
    fn remove_peer(&self, para_id: ParaId);
    fn all_peers(&self) -> Vec<(ParaId, OpaquePeerId)>;
}
```

### HardcodedPeerRegistry (MVP implementation)

- In-memory `HashMap<ParaId, OpaquePeerId>` with `RwLock`
- Populated from pallet storage (`RelayPeers`) via root-only extrinsics
- Opaque peer IDs are serialized `libp2p::PeerId` bytes
- Max peer ID length: 64 bytes

### Peer Registration Flow

```
1. Operator runs sudo extrinsic:
   SpeculativeMessaging::set_relay_peer(dest_para_id, peer_id_bytes)

2. Pallet stores in RelayPeers storage map

3. Collator client reads RelayPeers and populates HardcodedPeerRegistry

4. When sending to dest_para_id:
   registry.get_peer(dest_para_id) -> Some(relay_peer_id)
   Send ForwardMessageRequest to relay_peer_id
```

---

## SpeculativeMessagingWorker

### Configuration

```rust
pub struct ServiceConfig {
    pub para_id: Option<ParaId>,   // None for pure relay peers
    pub role: NodeRole,
}

pub enum NodeRole {
    Collator,     // Produces and consumes messages
    RelayPeer,    // Forwards messages between collators
}
```

### Core Methods

#### distribute_outgoing(batches) -> Vec<(ParaId, Result<()>)>

Called after block production. Sends message batches to destination relay peers.

```
For each (dest_para, batch) in batches:
  1. Look up relay peer: registry.get_peer(dest_para)
  2. Construct ForwardMessageRequest { source_para, dest_para, batch }
  3. Send via NetworkTransport::send_request(peer, request)
  4. Collect result per destination
```

#### drain_incoming() -> Vec<MessageBatch>

Called during block authoring. Drains all validated message batches accumulated since the last drain.

```rust
pub fn drain_incoming(&self) -> Vec<MessageBatch> {
    let mut incoming = self.incoming_batches.lock();
    std::mem::take(&mut *incoming)
}
```

Thread-safe via `Arc<parking_lot::Mutex<Vec<MessageBatch>>>`.

#### run() — Main Event Loop

```
loop {
    select! {
        request = incoming_requests.next() => {
            match self.role {
                NodeRole::Collator => handle_as_collator(request),
                NodeRole::RelayPeer => handle_as_relay_peer(request),
            }
        }
    }
}
```

### Message Handling Roles

#### handle_as_collator(request)

```
1. Verify request.destination_para == our para_id
   If not: Reject("not our destination")

2. Verify subtree inclusion proof:
   batch.verify_subtree_inclusion(our_para_id)
   If invalid: Reject("invalid proof")

3. Queue for block authoring:
   incoming_batches.lock().push(batch)

4. Respond: ForwardMessageResponse::Accepted
```

#### handle_as_relay_peer(request)

```
1. Look up destination's relay peer:
   registry.get_peer(request.destination_para)
   If not found: Reject("no peer for destination")

2. Forward the request to the destination's relay peer:
   transport.send_request(dest_peer, request)

3. Respond: ForwardMessageResponse::Forwarded
```

---

## Network Transport Abstraction

```rust
pub trait NetworkTransport: Send + Sync + 'static {
    async fn send_request(
        &self,
        peer: &OpaquePeerId,
        request: ForwardMessageRequest,
    ) -> Result<ForwardMessageResponse, Error>;
}
```

Abstracts the actual libp2p request-response protocol. Allows mock implementations for testing.

---

## Outbound Distribution (Collator Side)

**Location:** `cumulus/test/service/src/lib.rs` (test service integration)

The outbound distributor watches for **best blocks** (not finalized — this is the "speculative" part):

```rust
let mut import_stream = outbound_client.import_notification_stream();

while let Some(notification) = import_stream.next().await {
    if !notification.is_new_best {
        continue;  // Skip reorgs and non-best blocks
    }

    // Read PendingOutgoing storage from the new best block
    // Construct MessageBatch for each destination
    // Generate subtree_inclusion_proof from TopLevelTree
    // Call worker.distribute_outgoing(batches)
}
```

**Why best block, not finalized:**
- Waiting for finality adds 2-3 relay blocks of latency
- Speculating on best block achieves ~1 relay block latency
- Relay chain provides/requires matching is the safety net
- If the source block is reverted, the provides root won't match

---

## Inbound Processing (Collator Side)

```rust
// During block authoring:
let incoming = worker.drain_incoming();

// Convert to inherent data:
let entries: Vec<(ParaId, u64, H256)> = incoming.iter().map(|batch| {
    (batch.source, batch.messages.len() as u64, batch.provides_root)
}).collect();

// Provide as inherent:
SpecMsgInherentDataProvider::new(entries)
```

The inherent triggers `receive_messages_inherent` in the pallet.

---

## Error Types

```rust
pub enum Error {
    NoPeerForPara(ParaId),       // No relay peer registered for destination
    SendFailed(String),           // Network request failed
    ReceiveFailed(String),        // Response read failed
    Rejected(String),             // Peer rejected the batch
    InvalidBatch(String),         // Incoming batch failed validation
    Codec(codec::Error),          // SCALE encoding/decoding error
}
```

---

## Diagram: Full Networking Flow

```
ParaA Collator                 Relay Network                 ParaB Collator
==============                 =============                 ==============

1. Block produced
   (best block, not finalized)
        |
2. Read PendingOutgoing[ParaB]
   from ParaA state
        |
3. Generate MerkleProof
   (subtree inclusion)
        |
4. Construct MessageBatch
   { source: A, provides_root,
     subtree_root, proof, msgs }
        |
5. ForwardMessageRequest -----> RelayPeerA
   { source: A, dest: B,          |
     batch }                       |
                              6. Lookup ParaB peer
                                   |
                              7. Forward request ----> RelayPeerB
                                                          |
                                                     8. Forward to
                                                        ParaB collator
                                                          |
                                                     9. verify_subtree_inclusion()
                                                          |
                                                     10. Queue in
                                                         incoming_batches
                                                          |
                                                     11. Next block author:
                                                         drain_incoming()
                                                          |
                                                     12. Create inherent
                                                         (source, count, root)
                                                          |
                                                     13. receive_messages_inherent()
                                                          |
                                                     14. PendingRequires updated
                                                          |
                                                     15. on_finalize -> commitments
```

---

## Verification Responsibilities

| Layer | What It Verifies |
|-------|-----------------|
| Collator (receiver) | Subtree inclusion proof (Merkle proof that subtree_root is in provides_root) |
| Pallet | Message count > 0, sequential positions, source limits, no duplicate source per block |
| PVF | Late block proofs (if timing mismatch), all proofs consumed |
| Relay chain | requires.expected_root == IncludedProvidesRoots[source].root |

Message **content** is verified implicitly: the MMR leaf hashes are committed in the provides root. If messages were tampered with, the subtree inclusion proof would fail.

---

## MVP Limitations

- **No dynamic discovery** — peers must be manually registered via sudo
- **No acknowledgement** — sender doesn't know if receiver processed messages
- **No retry/redelivery** — if a batch is lost, it's lost
- **Single relay peer per destination** — no redundancy
- **No rate limiting** — relies on pallet bounds (MaxMessagesPerBlock, etc.)
