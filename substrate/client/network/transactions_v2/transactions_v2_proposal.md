## `transactions/2` proposal

This document outlines the high level design of a new transaction protocol. It collects and summarizes ideas discussed in the past in numerous issues.

### High-Level Summary

The current Polkadot/Substrate transaction propagation protocol (`transactions/1`) floods full transaction bodies to every connected peer, causing excessive bandwidth consumption and notification-channel congestion under load. The `transactions/2` protocol replaces this with a **descriptor-first** approach:

1. **Compact descriptors instead of full bodies.** When a transaction is validated and enters the local pool, a small *transaction descriptor* (typically a 32-byte hash) is broadcast to all connected peers via a notification sub-protocol (`TxIF` - Transaction Identifiers Flooding). This dramatically cuts per-peer bandwidth.

2. **On-demand body fetching.** Peers that receive a descriptor they have not seen before request the full transaction body through a dedicated request-response sub-protocol (`TxRR` - Transaction Request-Response), fetching only what they need.

3. **Optional latency optimization.** A low-fanout strategy (`TxLF`) can send full transaction bodies to a small, randomly selected subset of peers to reduce first-hop latency, similar to Ethereum's `eth` protocol.

4. **Path to set reconciliation.** The descriptor format includes a 32-bit fingerprint variant that lays the groundwork for *PinSketch*-based set reconciliation (as proposed in Bitcoin's *Erlay*), enabling periodic efficient synchronization of transaction sets between peers with minimal bandwidth.

5. **Backwards compatible.** Nodes supporting both `transactions/1` and `transactions/2` fall back to the legacy protocol only when talking to peers that do not support v2.

The protocol is rolled out in phases: Phase 1 delivers the core descriptor/flooding/request-response pipeline, Phase 1a adds low-fanout, Phase 1b adds authority-matrix gossiping, and Phase 2 introduces 32-bit fingerprints and set reconciliation.

---

### TL;DR

The new transaction protocol streamlines the process of handling transactions to improve efficiency and reduce network congestion. When a transaction is submitted and validated, a compact transaction descriptor is created and flooded to connected peers instead of the full transaction body. This significantly reduces bandwidth usage. Nodes receiving the descriptor request the full transaction data from peers using a request-response protocol, ensuring that only necessary data is transmitted.

To further optimize the process, the protocol includes future optional enhancements such as a low-fanout strategy, where full transaction data is relayed to a select group of peers, and set reconciliation techniques to synchronize transaction sets across the network periodically. The protocol is designed to be interoperable with existing systems, supporting both old and new protocols for a smooth transition, and also allows future extensions.


### Problems with Current Implementation
- High bandwidth usage due to transaction bodies being gossiped across all peer pairs.
- Synchronous notification network channels becoming clogged when a high number of transactions are dumped into the network, causing peer disconnections.

### Current Metrics
In tests where 1000 transactions were submitted to a relay-chain built from 20+1 nodes, the median propagation time was 1 second. Maximum propagation times varied between 1.5 and 3.3 seconds over 10 trials.

### Proposed Solutions
1. **Increase Transactions per Network Notification:** This approach, demonstrated in a [Proof of Concept](https://github.com/paritytech/polkadot-sdk/pull/7828), proposes increasing the number of transactions in a single network notification to reduce congestion.
2. **Introduce a New Transaction Protocol:** A more comprehensive solution may involve the development of a new transaction protocol to address the identified issues. This was discussed and proposed many times e.g. in [#6433](https://github.com/paritytech/polkadot-sdk/issues/6433).

### Prior Work
In the past an introduction of reconciliation set into Polkadot transaction protocol was mentioned many times. This approach was initially evaluated in *Erlay*.

*Erlay* is a proposed enhancement to the Bitcoin protocol designed to optimize the bandwidth used during the transaction broadcast process. The protocol introduces a method where transaction hashes are initially shared with a limited set of peers, a phase known as *low fanout*. Following this, nodes request the full transaction data using a `GETDATA` message. Crucially, *Erlay* periodically triggers a set reconciliation process to synchronize transaction sets between peer pairs.

While *Erlay* significantly reduces bandwidth consumption, it introduces an increase in transactions propagation latency. This trade-off needs careful consideration. Teams in Polkadot ecosystem may have different requirements and some flexibility is required in transaction protocol.

For more detailed insights, consider the following resources [[1](https://delvingbitcoin.org/t/erlay-overview-and-current-approach/1415)],[[2](https://arxiv.org/pdf/1905.10518)](sections 1-5).

### Components of new transaction protocol

#### Transaction Descriptor
The new transaction protocol introduces a flexible transaction descriptor format, living only on the network layer, supporting multiple identification schemes (including full transaction body), enabling seamless future protocol extensions with minimal rework.

Proposed descriptor format (`TxDescriptor`):
- The descriptor begins with a format specifier, allowing various types of payload:
  - `0`: transaction body v1: `leb128(size tx body) ++ scale-encoded tx-body` (LEB128-encoded size followed by SCALE-encoded transaction body, enabling direct transmission of the full transaction body),
  - `1`: 32-byte hash v1 (currently used),
  - `2`: 32-bit transaction fingerprint v1, allowing further bandwidth optimizations, and laying the ground for *PinSketch* based set-reconciliation implementation as defined in *Erlay*.

Two latter payloads (and possible extensions in future) are referred as transaction identifiers (`TxIdentifier`).

It is worth noting that all nodes must handle both transaction hashes (1) and bodies (0) within descriptors to maintain basic protocol functionality.

#### Transactions Identifiers Flooding (`TxIF`)
Transaction identifiers are gossiped to all connected peers (except _LightNodes_) allowing many transaction descriptors in a single networking notification. This reduces the required bandwidth and the depth of queues in networking module implementation. Identifiers are transmitted after a transaction is submitted to the local pool and validated as ready.

The transaction descriptor shall not be sent to a peer that already knows its identifier, whether because we received it from that peer or previously sent the transaction descriptor to it. This behavior is punishable by decreasing the peer's reputation. Known identifiers shall be kept for T seconds, with a maximum of N identifiers per peer. Networks should configure these limits based on their transaction volumes, and node network and memory requirements. Since the transaction pool can track transactions that are dropped or finalized, these notifications can also be leveraged to maintain internal structures.

Once a transaction descriptor is received, and it is not a transaction body, the latter shall be downloaded using the `TxRR` protocol (see the next section) and imported to the local pool. If a received transaction descriptor contains the transaction data it shall be imported to the pool.

If the transaction associated with received descriptor is found to be invalid, the reputation of peer gossiping invalid descriptor shall be decreased.

_Notes_:
- some measures shall be taken to avoid sending single identifier in notification. The [pool import notification stream](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/service/src/builder.rs#L593) shall be drained (maybe with some reasonable delay - 30-50ms to speculatively allow more transactions to come).
- when the combined size of all identifiers to be gossiped is relatively small compared to the network packet size, the network notification can include some of the transaction bodies to reduce the latency required for transaction dissemination. It is up to implementation to decide what is the most efficient approach.

#### Transaction Data Request-Response Protocol (`TxRR`)
After receiving a transaction identifier, the node should request the transaction body from a random peer which gossiped the transaction identifier, if the transaction is not already in the local pool, and there is not a pending request for it. Requests should have a short timeout to avoid denial of service where peers only gossip identifiers (and never provide their bodies). The timeout shall have constant base (e.g. 500ms) and a component proportional to the number of transactions.

The response must include transaction bodies in the same order they were requested. If a transaction body cannot be delivered, an indicator that the transaction was not available should be included in its place.

If the requested transaction is not available in the local pool the requesting peer's reputation shall be decreased. However, if the peer requests the transaction whose identifier was previously gossiped to that peer and transaction is found to be unknown (e.g. due to finalization or being dropped) its reputation shall remain unaffected. This kind of race condition is possible and there is little that can be done to prevent it.

Once transaction bytes are downloaded the transaction should be sent to pool for further validation and processing.

If a remote node is unable to provide a transaction it previously announced through identifiers flooding, the requesting node shall decrease its reputation. As noted in the previous paragraph, such situations may occur but should not be common. Reputation of nodes that gossip identifiers without being able to provide the corresponding transaction bodies should be decreased.

The request protocol supports batch acquisition of transactions by accepting a `Vec<TxIdentifier>`.


_Notes_:
- In theory, the transaction body could be fetched only if there is an available space in transaction pool. Transaction shall not be silently dropped. On the other hand the gossiped transaction identifier may correspond to transaction with higher priority and we should submit such transaction immediately as it may be evicting some other lower-priority transactions. To avoid downloading same transaction multiple times, the implementation shall keep track of downloaded transactions. A txpool API could be leveraged for this (see implementation notes).
- The node could retain the bodies of transactions that were broadcasted and then dropped from the pool. This would prevent its reputation from being decreased due to an inability to provide a transaction body when the pool is under high pressure and operating at its limits.

#### [Optional] Transaction Data Low-Fanout (`TxLF`)
The low-fanout strategy is an optional, easily achievable enhancement aimed at improving transaction propagation latency and network resilience.

The transaction descriptors containing full transaction data are relayed to a small number of (randomly / based on reputation) selected peers. This approach is taken in [Ethereum protocol](https://github.com/ethereum/devp2p/blob/master/caps/eth.md#transaction-exchange). When a transaction is submitted via RPC to the local node, the full transaction data may be relayed to an increased number of peers, as no other nodes initially possess this data. This approach helps decrease propagation time by ensuring quicker dissemination.


Multiple transaction descriptors shall be batched into a single network notification.

This extension could be used to quickly broadcast transactions from the light nodes to the network.

_Notes_:
- when a local pool is full the tx can be dropped (e.g. due to lower priority) and there is little we can do about this.
- `TxLF` only requires extra implementation on the sending side - handling transaction body in `TxDescriptor` is mandatory so no extra support is required.


#### [Optional] Authorities matrix for exchanging transactions
Not necessarily a part of protocol itself, could be considered as the optimization of implementation. Authorities could use matrix to effectively exchange txs. Doing so may decrease the latency and improve pools alignment. Similar approach was taken in `polkadot-gossip-support` module ([code](https://github.com/paritytech/polkadot-sdk/blob/98c6ffcea6794d338514cf9bd84446d2f276cb63/polkadot/node/network/gossip-support/src/lib.rs#L786), [doc](https://web.archive.org/web/20221210090830/https://research.web3.foundation/en/latest/polkadot/networking/3-avail-valid.html)). This would require authority-discovery on parachains and probably more thinking about how it could work.

### [Optional] Towards set-reconciliation
This section outlines potential future extensions to the transaction protocol. The details of these improvements require further consideration, particularly regarding the cooperation of nodes with different enabled features within the same network. Also some details need to be figured out - this section only provides potential directions of future work.

#### [Optional] 32-bit identifier
During the handshake peers shall exchange salts. Later salts shall be used to generate 32-bit transaction identifiers that are only valid for a given pair of peers. This allows to reduce the size of data being flooded to the network, and lays out the foundation for introducing a set sketch based reconciliation.


#### [Optional] Transaction identifiers low fanout + set-reconciliation
- The number of peers in fanout should be configurable,
- How/when to select peers?
- Periodic set-reconciliation between peers. (For early evaluation a naive set-reconciliation could be implemented).


#### [Optional] Naive set-reconciliation
Purpose of this exercise is to evaluate the benefits of applying set-reconciliation before diving into a full implementation. Instead of using sketch for computing differences the whole set of transaction identifiers is sent for computing a diff.
- Send vec of all known tx ids to every peer,
- When the set of other peer's txs is received compute the difference against known txs in a local pool,
- send the difference to the peer,
- fetch unknown transactions using `TxRR`

#### [Optional] *PinSketch* based set-reconciliation
Use *PinSketch* ([mini-sketch](https://github.com/bitcoin-core/minisketch) lib implemented for *Erlay*) to compute the set difference.

### Protocol metrics
At least following protocol metrics shall be implemented:
- invalid transactions with labels: reason + peer,
- peer reputation adjustments shall be trackable in logs,
- number of txs in/out (peer label maybe?),

### Protocol Handshake
To ensure future compatibility and extensibility, the protocol handshake process should include a mechanism for nodes to declare their capabilities. This allows peers to communicate the specific features they support, facilitating future upgrades.
During the handshake, nodes should exchange a set of capabilities, including:

- **Support for `TxIdentifier`:** Nodes should declare what `TxIdentifier` (e.g. 32-bit fingerprints) they support,
- **Set Reconciliation Versions:** Nodes should specify the versions of set reconciliation they support, as multiple versions are planned,

By including these capability declarations in the handshake, the protocol remains flexible and adaptable to future enhancements, ensuring that nodes can efficiently communicate their supported features and maintain compatibility across the network.

### Interoperability
Nodes supporting both `transactions/1` and `transactions/2` protocols shall only use `transactions/2` for communication. Node supporting both versions of protocol should use `transactions/1` only to communicate with nodes supporting `transactions/1`. This most likely requires some changes in implementation of `transactions/1` protocol.

### Sync overview

#### Accepting transaction descriptors
Protocol shall have a dedicated message indicating the readiness to accept the transaction descriptors. This feature can be useful during major sync to inform other peers that no descriptors shall be transmitted to the given node.

_Note_: could we simply disconnect from `transactions/2` instead of having enabled/disabled state?

#### Initial sync
All ready transaction descriptors shall be transmitted to newly connected peer once it declares readiness to receive descriptors.

#### Regular sync
Regular sync is achieved by the following means:
1. a node shall broadcast transaction descriptors (`TxIF/TxLF`) for all imported ready transactions. Some delay could be introduced to allow batching many transactions into single notification,
2. a node can periodically (e.g. block time) trigger broadcast of transaction descriptors (`TxIF`) for transactions that were not yet transmitted to given peer,
3. a node can broadcast all descriptors (`TxIF`) for ready transactions when new peer is connected and declares readiness of accepting transaction descriptors.

It should be enough to implement (1+2) or (1+3).

_Note_:  I currently don't see the reason why we would need to have (2). In case we have initial sync + notification of all imported ready transactions (1+3) all peers should receive the entire set of transactions.

### Notes: implementation guidelines
- re-use (copy) the existing [`transactions/1`](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs) protocol, [instantiation](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/service/src/builder.rs#L1067-L1074) can be done in similar manner, CLI arg should be exposed,
- all sync peers shall [join](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs#L378-L401) transaction protocol,
- two approaches for triggering broadcasts:
  - periodic / tx-import notification driven broadcast. Periodic is needed to feed the peers not connected during import event. (current [implementation](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs#L295))
  - on-peer-connection / tx-import notification driven broadcast also could be implemented (more reasonable?). The newly connected
- [`ready_transactions`](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/transaction-pool/api/src/lib.rs#L339) would be nice in TransactionPool API,
- a `Vec<TxDescriptor>` on notification protocol and `Vec<Transaction>` in `TxRR`,
- `Vec<TxDescriptor>` for notification protocol decodes as scale-encoded
- input of `TxRR` would be: input (scale-encoded `Vec<Hashes>` )
- output response of `TxRR` would be: `(leb128(size tx body) ++ scale-encoded tx-body) ++ ...++ (leb128(size tx body) ++ scale-encoded tx-body)`
- `TxRR`: Implementation could be inspired by existing (e.g. [beefy](https://github.com/paritytech/polkadot-sdk/blob/0404a8624964441011730e274c7a02972b63245c/substrate/client/consensus/beefy/src/communication/request_response/mod.rs)) request response protocol.
- a txpool could expose the API to check if given transaction hash is banned. This could be used as a guard to prevent downloading transactions that were already dropped from the pool.

### Roll-out plan
- Phase 1:
  - introducing a transaction descriptor format,
  - using transaction identifiers flooding (`TxIF`),
  - The `TxRR` (tx request response) for requesting transaction bodies,

- Phase 1a:
  - low fanout flooding for bodies (`TxLF`),

- Phase 1b:
  - matrix based gossiping between authorities,

- Phase 2:
  - 32 bit transaction fingerprint,
  - include low-fanout strategies and set reconciliations:
    - research: naive implementation to evaluate the impact to latencies/bandwidth,
    - full implementation

---

### Detailed Implementation Plan

This section breaks the roll-out phases into concrete, actionable implementation steps.

#### Phase 1 — Core Protocol

##### Step 1: Define wire types and codec (`sc-network-transactions-v2-types`)
- Define `TxDescriptor` enum with format specifier byte:
  - Variant `0` (Body): LEB128 length-prefixed, SCALE-encoded transaction body.
  - Variant `1` (Hash): 32-byte transaction hash.
- Define `TxIdentifier` as a subset of `TxDescriptor` (hash-only for Phase 1).
- Define request/response types for `TxRR`:
  - Request: SCALE-encoded `Vec<TxIdentifier>`.
  - Response: sequence of `Option<TxBody>` entries (LEB128-prefixed bodies, or a zero-length marker for unavailable transactions), preserving request order.
- Define handshake type carrying a capability bitfield (initially: supported `TxIdentifier` variants).
- Add SCALE `Encode`/`Decode` derives and unit tests for round-trip encoding of every type.

##### Step 2: Create the `transactions/2` notification sub-protocol
- Create a new crate `sc-network-transactions-v2` under `substrate/client/network/transactions_v2/`, modelled on the existing `transactions/1` crate (`substrate/client/network/transactions/`).
- Register a new notification protocol named `transactions/2` with `NotificationService`.
- Implement the protocol handshake exchanging the capability bitfield defined in Step 1.
- On the sending side: accept a `Vec<TxDescriptor>` and SCALE-encode it into a single notification message.
- On the receiving side: decode incoming notification into `Vec<TxDescriptor>`, dispatch each descriptor:
  - Body variant -> submit directly to the transaction pool.
  - Hash variant -> queue for fetching via `TxRR` (Step 3).

##### Step 3: Implement `TxRR` request-response sub-protocol
- Add a new request-response protocol configuration (inspired by [beefy req-res](https://github.com/paritytech/polkadot-sdk/blob/0404a8624964441011730e274c7a02972b63245c/substrate/client/consensus/beefy/src/communication/request_response/mod.rs)).
- Request handler (responder side):
  - Look up each requested `TxIdentifier` in the local transaction pool.
  - Return bodies in request order; use an empty marker for transactions that are no longer available.
  - Reputation: if the requester asks for a hash that was never announced to them, decrease their reputation.
- Request issuer (requester side):
  - On receiving a hash descriptor from `TxIF`, check: (a) not already in the local pool, (b) not already in-flight.
  - Pick a random peer from those who announced the hash.
  - Set timeout = `500ms + 10ms * num_requested_txs`.
  - On response: submit each received body to the transaction pool for validation.
  - On timeout or peer failure: retry with a different announcing peer; decrease the original peer's reputation.

##### Step 4: Per-peer known-descriptor tracking
- Maintain a per-peer bounded set of recently sent/received `TxIdentifier`s (bounded by N entries, entries expire after T seconds).
- Before sending a descriptor to a peer, check whether it is already in their known set; skip if so.
- When receiving a descriptor that was already sent to the peer, decrease the sender's reputation.
- Integrate with transaction pool finality/drop notifications to prune identifiers from the known sets.

##### Step 5: Notification batching and drain strategy
- Instead of sending one notification per imported transaction, drain the pool import notification stream with a configurable delay (default 30-50ms) to batch multiple descriptors into a single notification.
- When the combined size of queued descriptors is small relative to the notification packet size, opportunistically include some full transaction bodies (variant `0`) to reduce round-trip latency.

##### Step 6: Sync state management
- Add a protocol-level "ready to receive descriptors" message so that nodes performing major sync can suppress incoming descriptors.
- On peer connection + readiness: transmit descriptors for all currently ready transactions in the local pool (initial sync).
- On each pool import event: broadcast new descriptors to all ready peers (regular sync, strategy 1+3 from the proposal).

##### Step 7: Wire up to `sc-service` and expose CLI flags
- Instantiate the `transactions/2` protocol in [`sc-service::builder`](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/service/src/builder.rs#L1067-L1074) alongside `transactions/1`.
- Add a CLI flag (e.g. `--transactions-protocol-version`) to control which protocol(s) are enabled: `v1`, `v2`, or `both` (default: `both`).
- When both are enabled and a peer supports v2, use v2 exclusively; fall back to v1 for legacy peers.

##### Step 8: Transaction pool API extensions
- Add `is_banned(hash) -> bool` to the `TransactionPool` trait so the network layer can skip downloading known-bad transactions.
- Expose a `ready_transaction_hashes() -> Vec<Hash>` method for efficient initial-sync descriptor generation without cloning full bodies.

##### Step 9: Metrics and observability
- Register Prometheus metrics:
  - `transactions_v2_descriptors_sent_total` / `transactions_v2_descriptors_received_total` (by type: hash, body).
  - `transactions_v2_txrr_requests_total` / `transactions_v2_txrr_responses_total` (by outcome: success, timeout, not_found).
  - `transactions_v2_invalid_transactions_total` (by reason label).
  - `transactions_v2_peer_reputation_adjustments_total` (by direction: increase/decrease).
- Add structured log lines (`tracing`) for reputation adjustments with peer ID context.

##### Step 10: Testing
- Unit tests for all codec types (round-trip, malformed input).
- Unit tests for per-peer known-set logic (insert, expiry, dedup, prune-on-finality).
- Integration tests using `sc-network-test` harness:
  - Two-node descriptor propagation and body fetch.
  - Interoperability: v1-only node + v2-capable node coexistence.
  - Reputation decrease on invalid descriptor / missing body.
- Zombienet test: multi-node relay chain with `transactions/2` enabled, submit transactions, verify propagation latency and bandwidth reduction compared to v1 baseline.

#### Phase 1a — Low-Fanout (`TxLF`)

##### Step 11: Implement low-fanout sending
- On transaction import, select a small random subset of peers (configurable, e.g. `sqrt(num_peers)`) and send them the full-body descriptor (variant `0`) via the same notification protocol.
- For RPC-submitted transactions (where the local node is the origin), increase the fanout count to ensure fast initial dissemination.
- All remaining peers receive hash-only descriptors as in Phase 1.
- No receiver-side changes needed — body descriptors are already handled.

#### Phase 1b — Authority Matrix Gossiping

##### Step 12: Authority-aware peer selection
- Integrate with `authority-discovery` to identify peers that are current validators/collators.
- Build a connectivity matrix (similar to `polkadot-gossip-support`) for authority peers.
- Prioritize sending descriptors (and optionally full bodies) to authority peers first, reducing block-production latency.
- This step requires further design on how authority-discovery works for parachains.

#### Phase 2 — Fingerprints and Set Reconciliation

##### Step 13: 32-bit transaction fingerprint
- Extend the handshake to exchange per-peer salts.
- Add `TxDescriptor` variant `2`: 32-bit fingerprint computed as `truncate32(hash(salt_a ++ salt_b ++ tx_hash))`.
- Update per-peer known-set tracking to work with fingerprints.
- Use fingerprint descriptors for `TxIF` when both peers support it, falling back to 32-byte hashes otherwise.

##### Step 14: Naive set-reconciliation (research/evaluation)
- Implement periodic exchange of full `Vec<TxFingerprint>` between peer pairs.
- Compute set difference locally, request missing transactions via `TxRR`.
- Measure bandwidth savings and latency impact in zombienet tests.
- Use results to decide whether full *PinSketch* implementation is justified.

##### Step 15: *PinSketch* set-reconciliation
- Integrate the [minisketch](https://github.com/bitcoin-core/minisketch) library (or a Rust port).
- Replace the naive full-set exchange with sketch-based reconciliation.
- Reconciliation runs periodically (e.g. every block time) between peer pairs.
- Peers exchange sketches, compute symmetric difference, and fetch missing transactions via `TxRR`.

---

### Definitions

- *Transaction Descriptor (`TxDescriptor`)* – A flexible format for representing transactions on the network layer, supporting multiple identification schemes (full body, hash, fingerprint).
- *Transaction Identifier (`TxIdentifier`)* – A compact representation of a transaction, such as a hash or fingerprint, used for efficient propagation, embedded in `TxDescriptor`.
- *Transaction Body* – The full transaction data, transaction bytes.
- *Transaction Identifiers Flooding (`TxIF`)* – A mechanism for broadcasting transaction identifiers to peers instead of full transaction bodies.
- *Transaction Request-Response (`TxRR`)* – A protocol for requesting and receiving full transaction bodies when needed.
- *Low-Fanout Transaction Data (`TxLF`)* – An optional strategy where full transactions are shared with a small subset of peers instead of all nodes.


### References
- [1] https://delvingbitcoin.org/t/erlay-overview-and-current-approach/1415
- [2] https://arxiv.org/pdf/1905.10518
