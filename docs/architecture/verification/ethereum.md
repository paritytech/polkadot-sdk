# Ethereum

We have implemented a Proof-of-Stake (PoS) light client for the Beacon chain. This  client deprecates the older PoW light client we developed in 2020.

The beacon client tracks the beacon chain, the new Ethereum chain that replaced the Ethereum's Proof-of-Work consensus method around on 15 September 2022, called the Merge. The work we have done consists of the following parts:

* Beacon Client pallet
  * Force checkpoint
  * Submit (finalized header & sync committee update)
  * Submit execution header
  * Message verification
* Beacon Relayer
  * Sends data from a beacon node to the beacon client

## Concepts

### Before the Merge: Execution Layer

Before the Merge, the Ethereum chain as we know it existed in isolation in the sense that consensus was determined by the same chain, using Proof-of-Work (POW).

<figure><img src="../../.gitbook/assets/Screenshot 2022-10-19 at 16.09.41.png" alt=""><figcaption><p>Ethereum Chain before the Merge</p></figcaption></figure>

### After the Merge: Consensus Layer

After the Merge, the Beacon chain became the sole manner in which consensus is tracked on Ethereum. The Beacon chain is a separate chain that was launched on 1 December 2020 and has been running independently since then. On 15 September 2022, the original Ethereum chain's POW consensus method was disabled and the chain switched over to the Beacon chain for consensus. The original Ethereum chain is now often referred to as the Execution Layer and the Beacon chain as the Consensus Layer.

<figure><img src="../../.gitbook/assets/Screenshot 2022-10-19 at 16.07.23.png" alt=""><figcaption><p>Ethereum Chains after the Merge</p></figcaption></figure>

### **Snowbridge Beacon Client**

The Snowbridge beacon client is based on the [Altair Sync Protocol](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md) (often referred to as ALC - Altair Light Client). Although there has been [some criticism of the protocol](https://prestwich.substack.com/p/altair) and its security, the ALC protocol remains the best explored light client to track the Beacon chain with reasonable security. If you are interested in additional reading about the sync committee's security, please read [our analysis on the Polkadot Forum](https://forum.polkadot.network/t/snowforks-analysis-of-sync-committee-security/2712/8).

#### **Beacon Headers & Execution Headers**

The Snowbridge light client to track Ethereum consensus is implemented as an on-chain Beacon client, on the parachain. It is implemented as a Substrate pallet and the code can be found on Github under the [`ethereum-beacon-client` pallet](../../../parachain/pallets/ethereum-beacon-client/src/lib.rs).

The beacon client tracks finalized beacon blocks. The Beacon chain introduced finality to the chain (more on this later). Since it is vital that transfer messages are included in the canonical chain (and not in blocks that go through a re-org), the beacon client only tracks blocks that are ancestors of finalized beacon blocks.

In the diagram below, the purple blocks are examples of those stored in the beacon client. Only finalized beacon blocks are stored as checkpoints. Not all finalized beacon blocks need to be stored and skipping a finalized block is allowed, since these finalized blocks are merely used as checkpoints to indicate that all ancestors of such a block will be seen as finalized as well.

Beacon blocks and execution headers are linked through the `ExecutionPayload` field in a Beacon block. To verify messages, we are particularly interested in the `receiptsRoot` hash, which is used to verify the Ethereum message receipt containing the details about the transfer. For this reason, we store all the execution headers that are ancestors of a finalized beacon header.&#x20;

<figure><img src="../../.gitbook/assets/Screenshot 2022-10-19 at 16.12.09.png" alt=""><figcaption><p>Snowbridge storage (items in purple are stored on-chain)</p></figcaption></figure>

#### Sync Committees

Additionally, the beacon client also syncs sync committees. Sync committees are a subset of randomly chosen validators to sign blocks for a sync committee period (256 epochs, around 27 hours).

<figure><img src="../../.gitbook/assets/Screenshot 2022-10-19 at 16.15.49.png" alt=""><figcaption></figcaption></figure>



### Proofs

The Beacon client checks the following proofs before storing beacon headers and execution headers:

* Merkle proof of the beacon state root to verify if the supposedly finalized header is finalized
* BLS signature verification to assert that the sync committee signed the block attesting to the finalized header
* Ancestry proofs to verify that the imported execution header is indeed a valid ancestor of a finalized header (also merkle proofs).

Additionally, the sync committee and next sync committee is also verified using Merkle proofs, to verify if those sync committees are part of the beacon state.

## Beacon Client Operations

### **Force checkpoint**

This operation can only be executed by the root origin (on pallet initialization or by governance) and serves a starting point for syncing blocks.&#x20;

The `force_checkpoint` payload contain:

* A beacon header (validated manually to ensure it is on the correct chain).
* The current sync committee plus a merkle proof branch to verify the sync committee.
* The validators root (the merkle root of all the validators that were present at genesis time - this is used to determine the correct chain).
* The block roots merkle root (the merkle root of the `blocks_root` field in the beacon state of the beacon header - used for ancestry proofs using the beacon header in this payload) plus the merkle branch roots to proof the blocks root merkle root against the beacon header state root.

### **Submit**

After the checkpoint has been validated, the beacon relayer periodically sends updates. These updates contain finalized headers and optionally, the next sync committee.&#x20;

The `submit` update contains:

* An attested header: A recent header attesting to the finalized header in the update. This header is not finalized, but its `state_root` field is used to prove the `finalized_header` field in the same update. This header isn't stored (because we are not interested in headers that are not finalized), but only used for proofs.
* A sync aggregate: The signing information concerning the attested header (the sync committee signature and voting information regarding the attested header, to see if we can trust it)
* The signature slot: The slot at which the sync committee signature for the attested header can be found. This is typically `attested_header.slot + 1`, unless the next slot is a skipped slot, in which case it will be `attested_header.slot + 2`, and so forth until a block at the slot is present (some slots contain no blocks and is called a missed block slot)
* The next sync committee update (optional): If the next sync committee is known and has not be stored in the beacon light client, the relayer will send it. The sync committee subset of validators change every \~27 hours. The sync committee is verified using a Merkle proof and then stored in storage.
* The finalized header and its merkle proof: This serves as a checkpoint to know which execution headers can safely be imported which being in danger of a reorg. The finalized block root header is stored along with the slot number and block roots root.
* The block roots root and its proof, similar to the force checkpoint update.

### **Execution header updates**

Once there are more than 2 beacon finalized headers, all the execution headers between the two finalized beacon headers are backfilled. The execution header lives on the Ethereum execution layer (historically just the Ethereum chain). The execution header looks almost the same as it used to in the Ethereum PoW world. Each beacon header contains an ExecutionPayload header which is on the execution layer. A compacted version of the execution header is stored in storage in order to use the `receipts_root` field for message verification.

The `submit_execution_header` update contains:

* A header: The beacon header containing an execution header.
* An ancestry proof: The merkle proof branch to the block\_root in the beacon state pointing to this header, plus the finalized header root used to proof this ancestor block.
* The execution header of this beacon header.
* The merkle proof to prove that this execution header is in fact contained in the header provided.

### **Message verification**

The light client is also responsible for verifying incoming Ethereum events. It does so using transaction receipt proofs which prove that a particular transaction to a particular Ethereum smart contract was in fact valid, was included in the chain, and did emit some event. It accepts and processes a proof, verifies it and then returns the set of Ethereum events that were emitted by the proven transaction receipt.

## Implementation

Pallets:

* [ethereum-beacon-client](https://github.com/Snowfork/snowbridge/tree/main/parachain/pallets/ethereum-beacon-client)
