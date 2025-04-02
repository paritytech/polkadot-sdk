## `transactions/2` proposal
This document outlines the high level design of a new transaction protocol. It collects and summarizes ideas discussed in the past in numerous issues.

### TL;DR
- Phase 1:
  - introducing a transaction descriptor format,
  - using transaction identifiers flooding,
  - The TxRR (tx request response) for requesting transaction bodies,

- Phase 1a:
  - low fanout floding for bodies,
  - matrix based gossiping between authorithies,

- Phase 2:
  - 32 bit transaction fingerprint,
  - include low-fanout strategies and set reconciliations:
    - research: naive implemenation to evaluate the impact to latencies/bandwidth,
    - full implementation

### Problems with Current Implementation.
- High bandwidth usage due to transaction bodies being gossiped across all peer pairs.
- Synchronous notification network channels becoming clogged when a high number of transactions are dumped into the network, causing peer disconnections.

### Current Metrics.
In tests where 1000 transactions were submitted to a relay-chain built from 20+1 nodes, the median propagation time was 1 second. Maximum propagation times varied between 1.5 and 3.3 seconds over 10 trials.

### Proposed Solutions.
1. **Increase Transactions per Network Notification:** This approach, demonstrated in a [Proof of Concept](https://github.com/paritytech/polkadot-sdk/pull/7828), proposes increasing the number of transactions in a single network notification to reduce congestion.
2. **Introduce a New Transaction Protocol:** A more comprehensive solution may involve the development of a new transaction protocol to address the identified issues. This was discussed and proposed many times e.g. in [#6433](https://github.com/paritytech/polkadot-sdk/issues/6433).

### Prior Work
In the past an introduction of reconciliation set into Polkadot transaction protocol was mentioned many times. This approach was initially evaluated in *Erlay*.

*Erlay* is a proposed enhancement to the Bitcoin protocol designed to optimize the bandwidth used during the transaction broadcast process. The protocol introduces a method where transaction hashes are initially shared with a limited set of peers, a phase known as *low fanout*. Following this, nodes request the full transaction data using a `GETDATA` message. Crucially, *Erlay* periodically triggers a set reconciliation process to synchronize transaction sets between peer pairs.

While *Erlay* significantly reduces bandwidth consumption, it introduces an increase in transactions propagation latency. This trade-off needs careful consideration. Teams in Polkadot ecosystem may have different requirements and some flexibility is required in transaction protocol.

For more detailed insights, consider the following resources [[1](https://delvingbitcoin.org/t/erlay-overview-and-current-approach/1415)](Sections 1-5),[[2](https://arxiv.org/pdf/1905.10518)].

### Components of new transaction protocol proposal.

#### Transaction Descriptor.
The new transaction protocol introduces a flexible transaction descriptor format, living only on the network layer, supporting multiple identification schemes (including full transaction body), enabling seamless future protocol extensions with minimal rework.

Proposed descriptor format (`TxDescriptor`):
- The descriptor begins with a format specifier, allowing various types of payload:
  - `0`: transaction body v1: `leb128(size tx body) ++ scale-encoded tx-body` (LEB128-encoded size followed by SCALE-encoded transaction body, enabling direct transmission of the full transaction body),
  - `1`: 32-byte hash v1 (currently used),
  - `2`: 32-bit transaction fingerprint v1, allowing further bandwidth optimizations, and laying the ground for *PinSketch* based set-reconciliation implementation as defined in *Erlay*.

Two latter payloads are referred as transaction identifiers.

#### Transactions Identifiers Flooding.
Transaction identifiers are gossiped to all connected peers (except _LightNodes_) allowing many transaction descriptors in a single networking notification. This reduces the required bandwidth and the depth of queues in networking module implementation.

The transaction descriptor shall not be sent to a peer that already knows its identifier, whether because we received it from that peer or previously sent the transaction descriptor to it. Known identifiers shall be kept for T seconds, with a maximum of N identifiers per peer. Networks should configure these limits based on their transaction volumes, and node network and memory requirements.

Once a transaction descriptor is received, and it is not a transaction body, the latter shall be downloaded using the TxRR protocol (see the next section).

Not part of the protocol, but maybe noteworthy here: some measures shall be taken to avoid sending single identifier in notification. The [pool import notification stream](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/service/src/builder.rs#L593) shall be drained (maybe with some reasonable delay - 30-50ms to speculatively allow more transactions to come).

Note: When the combined size of all identifiers to be gossiped is relatively small compared to the network packet size, the network notification can include some of the transaction bodies to reduce the latency required for transaction dissemination.

#### Transaction Data Request-Response Protocol (TxRR).
After receiving a transaction identifier, the node should request the transaction body from a random peer which gossiped the transaction identifier, if the transaction is not already in the local pool, and there is not a pending request for it. Requests should have a short timeout to avoid denial of service where peers only gossip identifiers (and never provide their bodies).

If the requested transaction is not available in the local pool the requesting peer's reputation shall be decreased. However, if the peer requests the transaction whose identifier was previously gossiped to that peer and transaction is found to be unknown (e.g. due to finalization or being dropped) its reputation shall remain unaffected. This kind of race condition is possible and there is little that can be done to prevent it. Once transaction bytes are downloaded the transaction should be sent to pool for further validation and processing.

If a remote node is unable to provide a transaction it previously announced through identifiers flooding, the requesting node shall decrease its reputation. As noted in the previous paragraph, such situations may occur but should not be common. Reputation of nodes that gossip identifiers without being able to provide the corresponding transaction bodies should be decreased.


The request protocol supports batch acquisition of transactions by accepting a `Vec<TxIdentifier>`.

Some open issue:
In theory, the transaction body could be fetched only if there is an available space in transaction pool. Transaction shall not be silently dropped. On the other hand the gossiped transaction identifier may correspond to transaction with higher priority and we should submit such transaction immediately as it may be evicting some other lower-priority transactions.

#### [Optional] Transaction Data Low-Fanout.
The low-fanout strategy is an optional, easily achievable enhancement aimed at improving transaction propagation latency and network resilience.

The transaction descriptors containing full transaction data are relayed to a small number of (randomly / based on reputation) selected peers. This approach is taken in [etheruem protocol](https://github.com/ethereum/devp2p/blob/master/caps/eth.md#transaction-exchange).

Multiple transaction descriptors shall be batched into a single network notification.

This extensions could be used to quickly broadcast transactions from the light nodes to the network.

When a local pool is full the tx can be dropped (e.g. due to lower priority) and there is little we can do about this.

Peer reputation shall be adjusted accordingly.

#### [Optional] Authorities matrix for exchanging transactions.
Not necessarily a part of protocol itself, could be considered as the optimization of implementation. Authorities could use matrix to effectively exchange txs. Doing so may decrease the latency and improve pools alignment. Similar approach was taken in `polkadot-gossip-support` module ([code](https://github.com/paritytech/polkadot-sdk/blob/98c6ffcea6794d338514cf9bd84446d2f276cb63/polkadot/node/network/gossip-support/src/lib.rs#L786), [doc](https://web.archive.org/web/20221210090830/https://research.web3.foundation/en/latest/polkadot/networking/3-avail-valid.html])). This would require authority-discovery on parachains and probably a more thinking how it could work.

### [Optional] Towards set-reconciliation.
This section outlines potential future extensions to the transaction protocol. The details of these improvements require further consideration, particularly regarding the cooperation of nodes with different enabled features within the same network. Also some details needs to be figure out - this section only provides a potential directions of future work.

#### [Optional] 32-bit identifier.
During the handshake peers shall exchange salts. Later salts shall be used to generate 32-bits transaction identifier that is only valid for given pair of peers. This allows to reduce the size of data being flooded to the network, and lays out foundation for introducing a set sketch based reconciliation.


#### [Optional] transaction identifiers low fanout + set reconciliation.
- The number of peers in fanout should be configurable,
- How/when to select peers?
- Periodic set-reconciliation between peers. (For early evaluation a naive set-reconciliation could be implemented).


#### [Optional] naive set-reconciliation.
Purpose of this exercise is to evaluate the benefits of applying set-reconciliation before diving into a full implementation. Instead of using sketch for computing differences the whole set of transaction identifiers is sent for computing a diff.
- Send vec of all known tx ids to every peer,
- When the set of other peer’s txs is received compute the difference against known txs in a local pool,
- send the difference to the peer,
- fetch unknown transactions using TxRR

#### [Optional] *PinSketch* based.
Use *PinSketch* ([mini-sketch](https://github.com/bitcoin-core/minisketch) lib implemented for *Erlay*) to compute the set difference.

### Protocol metrics.
At least following protocol metrics shall be implemented:
- invalid transactions with labels: reason + peer,
- peer reputation adjustments shall be trackable in logs,
- number of txs in/out (peer label maybe?),

### Protocol handshake
To maintain future backward compatibility within the protocol, it is advisable to minimize breaking changes. Given the anticipated future enhancements, the handshake process can be strategically designed to accommodate these developments. It is essential for peers to communicate the specific protocol features they support. Accordingly, a designated mechanism or placeholder should be incorporated to facilitate this exchange of capabilities.

### Interoperability
Nodes supporting both `transactions/1` and `transactions/2` protocols shall only use `transactions/2` for communication. Node supporting both versions of protocol should use `transactions/1` only to communicate with nodes supporting `transactions/1`. This most likely requires some changes in implementation of `transactions/1` protocol.


### Notes: implementation guidelines
- re-use (copy) the existing [`transactions/1`](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs) protocol, [instantiation](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/service/src/builder.rs#L1067-L1074) can be done in similar manner, CLI arg should be exposed,
- all sync peers shall [join](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs#L378-L401) transaction protocol,
- two approaches for triggering broadcasts:
  - periodic / tx-import notification driven broadcast. Periodic is needed to feed the peers not connected during import event. (current [implementation](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/network/transactions/src/lib.rs#L295))
  - on-peer-connection / tx-import notification driven broadcast also could be implemented (more reasonable?).
- [`ready_transactions`](https://github.com/paritytech/polkadot-sdk/blob/ec700de9cdca84cdf5d9f501e66164454c2e3b7d/substrate/client/transaction-pool/api/src/lib.rs#L339) would be nice in TransactionPool API,
- a `Vec<TxDescriptor>` on notification protocol and `Vec<Transaction>` in TxRR,
- `Vec<TxDescriptor>` for notification protocol decodes as scale-encoded
- input of TxRR would be: input (scale-encoded `Vec<Hashes>` )
- output response of TxRR would be: `(leb128(size tx body) ++ scale-encoded tx-body) ++ ...++ (leb128(size tx body) ++ scale-encoded tx-body)`
- TxRR: Implementation could be inspired by existing (e.g. [beefy](https://github.com/paritytech/polkadot-sdk/blob/0404a8624964441011730e274c7a02972b63245c/substrate/client/consensus/beefy/src/communication/request_response/mod.rs)) request response protocol.


### References
- [1] https://delvingbitcoin.org/t/erlay-overview-and-current-approach/1415
- [2] https://arxiv.org/pdf/1905.10518

