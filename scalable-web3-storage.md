# Scalable Web3 Storage

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Draft |
| **Version** | 1 |
| **Related Designs** | [Proof-of-DOT Infrastructure Strategy](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/) |

---

## Table of Contents

1. [Introduction](#introduction)
2. [Status Quo](#status-quo)
3. [Goals](#goals)
4. [Non-Goals](#non-goals)
5. [Solution Overview](#solution-overview)
6. [Proof-of-DOT Foundation](#proof-of-dot-foundation)
7. [Storage Model](#storage-model)
8. [Buckets and Storage Agreements](#buckets-and-storage-agreements)
9. [Read Incentives](#read-incentives)
10. [Discovery](#discovery)
11. [Data Model](#data-model)
12. [Multi-Provider Redundancy](#multi-provider-redundancy)
13. [Encrypted Data: Strong Guarantees Without Sealing](#encrypted-data-strong-guarantees-without-sealing)
14. [Use Cases](#use-cases)
15. [Comparison with Existing Solutions](#comparison-with-existing-solutions)
16. [Related Work](#related-work)
17. [Bootstrapping and Rollout](#bootstrapping-and-rollout)
18. [Operational Considerations](#operational-considerations)
19. [Future Directions](#future-directions)
20. [Summary](#summary)
21. [Appendix: Detailed Comparisons](#appendix-detailed-comparisons)

---

## Introduction

Existing decentralized storage systems put the blockchain in the hot path. Filecoin requires continuous on-chain proofs every 24 hours for every sector. StorageHub issues random challenges every block. This creates fundamental scalability limits: chain throughput bounds storage capacity.

We propose a different architecture: **the chain exists as a credible threat, not as the happy path**. In normal operation, clients and providers interact directly—no chain transactions for writes, reads, or ongoing storage. The chain is only touched for identity registration (once), payments to providers (simple transfers for priority service), availability commitments (batched, infrequent), and dispute resolution (rare, expensive, avoided by rational actors).

This works because of two foundations:

**Proof-of-DOT** provides sybil resistance and enables reputation. Clients lock DOT against a PeerID; providers track payment history per client. Service quality becomes a competition: providers who serve well attract paying clients, providers who don't get dropped. No complex payment channels needed—just simple transfers and local bookkeeping. The amounts are small (bandwidth costs ~€0.001/GB), so fine-grained payment accounting isn't worth the overhead.

**Game-theoretic enforcement** replaces continuous cryptographic proofs. Providers commit Merkle roots on-chain and lock stake. Clients can challenge at any time, forcing the provider to prove data availability or lose their stake. The challenge mechanism is expensive for everyone—which is the point. Rational providers serve data directly because being challenged costs them money even if they're honest. Rational clients don't challenge frivolously because it costs them too. The expensive on-chain path exists to make the cheap off-chain path incentive-compatible.

The result: storage that scales with provider capacity, not chain throughput. Writes are instant (when no consensus is needed). Reads are fast (direct from provider). Guarantees are optional and tiered—ephemeral data needs no on-chain commitment, critical backups get storage agreements with slashing.

This document details the design: storage model, buckets and storage agreements, read incentives, discovery, and how it compares to existing solutions.

---

## Status Quo

### The Write Problem

Filecoin was designed around sealed cold storage. The traditional path requires sealing data into sectors with cryptographic proofs—sealing a 32GB sector takes ~1.5 hours and requires specialized hardware (GPUs, CPUs with SHA extensions). 

Filecoin has evolved: [Proof of Data Possession (PDP)](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/), launched on mainnet in May 2025, provides a simpler alternative for hot storage without sealing. This moves in a similar direction to the design proposed here. PDP is a legitimate alternative worth considering, though it still requires periodic on-chain proofs and the complexity of Filecoin's deal infrastructure.

IPFS allows fast writes (just send data to a node), but provides no guarantee the data will persist. The node can drop it immediately or refuse to accept it.

Note here: I would consider the traditional Filecoin approach (which includes the Polkadot based Eiger implementation) completely unsuited for our needs. It is too slow and too heavy (32GB sectors) to serve hot storage demanded by interactive applications.

### The Storage Problem

Proving that data is stored is expensive. Filecoin uses Proof-of-Replication (PoRep) and Proof-of-Spacetime (PoSt), requiring continuous on-chain proofs. Every 24 hours, every sector is proven. This creates chain load and operational complexity for storage providers.

IPFS requires no proofs at all, meaning users have no assurance their data still exists, which becomes visible in the known poor user experience of IPFS.

### The Read Problem

Once you have proven data is available, you still need to be able to read it. Consider the data flow:

```
User wants image
    ↓
Query DHT to find providers (2-10 seconds)
    ↓
Connect to provider (variable, often fails)
    ↓
Download data (depends on provider's willingness)
```

The DHT lookup alone takes seconds. Providers have no incentive to serve quickly—or at all. They proved they *have* the data; nobody pays them to *serve* it. Popular content works because altruistic nodes cache it. Unpopular content may be technically "available" but practically unretrievable.

### The Latency Reality

| System | Write Latency | Read Latency | Storage Guarantee |
|--------|---------------|--------------|-------------------|
| Filecoin (cold) | Hours (sealing) | Hours (unsealing) | Strong (PoSt) |
| Filecoin (hot/PDP) | Deal negotiation | Seconds | Strong (PDP) |
| IPFS | Instant | Seconds (DHT lookup) | None |
| Arweave | ~2 minutes | Seconds | Permanent |
| Centralized (S3) | ~100ms | ~50ms | SLA-backed |

For IPFS, "seconds" is the happy path—when content is found. DHT lookups can fail entirely if providers are offline, unreachable (NAT/firewall), or simply haven't announced the content. There's no way to distinguish "content doesn't exist" from "content exists but I can't find it."

---

## Goals

1. **Write without consensus.** Data writes should not touch the chain. A user sending an image in a chat should experience sub-second latency with no on-chain transaction. On-chain transactions may be used to achieve consensus, where necessary.

2. **Optional, efficient storage guarantees.** Not all data needs the same guarantees. Ephemeral chat images don't need strong guarantees. Critical backups do. Users pay for what they need.

3. **Incentivized reads.** Providers must be economically motivated to serve data quickly, not just prove they have it. Read performance should improve with competition, not degrade with scale.

4. **Accountable providers.** When a provider commits to storing data, that commitment must be enforceable. Cheating must be detectable and punishable without requiring continuous on-chain proofs.

5. **Permissionless participation.** Anyone can become a storage provider. No gatekeepers, no special hardware requirements beyond storage capacity.

---

## Non-Goals

**Database-style access.** This design optimizes for file-like patterns: store blobs, retrieve by content hash, read ranges. Not for small random key-value lookups, indexes, or queries. You *can* build a Merkle trie on top (like blockchain state), but then you've built a blockchain's state layer—at which point, use a blockchain. 

**Permanent storage.** Unlike Arweave, we do not aim for "store once, available forever." Storage has a duration, contracts have terms. This is a feature: it allows for deletion, reduces costs, and matches how most applications actually use storage.

**Privacy at the protocol level.** Private data is encrypted data. The storage layer sees opaque bytes. Key management, access control, and encryption are application-layer concerns. In addition (not to be relied upon), we do introduce permissions and if read is restricted, storage providers should only serve data to clients listed with read permissions in the bucket. Obviously this can't be enforced, so for truly private data this should only be used in addition to encryption.

---

## Solution Overview

The design separates writes, storage, and reads into independent layers, connected by a common foundation: Proof-of-DOT for sybil resistance and identity.

```
┌─────────────────────────────────────────────────────────────────────┐
│                           ON-CHAIN                                  │
│                                                                     │
│    Only touched for:                                                │
│    • Proof-of-DOT registration (once per identity)                  │
│    • Credit transfers to providers (prepay for priority)            │
│    • Availability contract commits (batched, infrequent)            │
│    • Dispute resolution (rare, game-theoretic deterrent)            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ rare
                                    │
┌─────────────────────────────────────────────────────────────────────┐
│                          OFF-CHAIN                                  │
│                                                                     │
│   ┌─────────────┐      writes      ┌─────────────┐                  │
│   │   Client    │ ───────────────> │  Provider   │                  │
│   │             │ <─────────────── │             │                  │
│   └─────────────┘      reads       └─────────────┘                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │
                                    │ foundation
┌─────────────────────────────────────────────────────────────────────┐
│                      PROOF-OF-DOT                                   │
│                                                                     │
│   Sybil resistance, identity, priority access                       │
│   See: Proof-of-DOT Infrastructure Strategy                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### How Data Flows

A typical write-then-read flow, showing what touches the chain:

```
Timeline:
═════════════════════════════════════════════════════════════════════

t=0ms     Client writes chunks to provider
          • Off-chain, direct connection
          • Provider stores locally
          • Client receives acknowledgment
          Chain: nothing

t=100ms   Data is readable
          • Anyone can request from provider
          • Provider serves based on Proof-of-DOT priority
          Chain: nothing

t=1hour   Provider batches many writes, commits MMR root
          • Optional: only if client requested availability guarantee
          • One transaction can cover thousands of writes
          • Even availability guarantees don't strictly need the chain, but it
            is a good means to achieve consensus, especially when multiple storage
            providers are used
          Chain: one commit transaction (batched)

t=1week   Client wants to verify data still exists
          • Requests random chunks from provider
          • Provider serves them, client verifies against known root
          Chain: nothing (off-chain verification)

t=never   Dispute (only if provider cheats)
          • Client challenges on-chain
          • Provider must submit Merkle proofs
          • Failure = full slash
          Chain: dispute transaction (rare, expensive, avoided)
```

The chain exists as a credible threat, not as the happy path. Rational providers serve data directly because the alternative (on-chain dispute) is expensive for everyone and catastrophic for cheaters.

---

## Proof-of-DOT Foundation

Before discussing storage mechanics, we need identity and sybil resistance. Without it, reputation is meaningless, spam is free, and accountability (for clients) is impossible.

Proof-of-DOT (detailed in the [Proof-of-DOT Infrastructure Strategy](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/)) provides this foundation:

**For clients:**
- Lock DOT against a PeerID
- Providers can quickly lookup PeerIDs for proof of DOT on connection establishment
- Enables reputation: providers remember past interactions with this identity
- Prevents spam at scale: creating identities gets costly for spammers

**For providers:**
- Same Proof-of-DOT mechanism as clients (sybil resistance, identity)
- *Separately*: providers lock collateral for storage agreements (see [Buckets and Storage Agreements](#buckets-and-storage-agreements))
- Collateral stake-per-byte ratio signals commitment level

**For priority access:**
- Providers track cumulative payments received from each client
- Clients who have paid more get priority in serving queues
- This is *payment history*, not stake amount—stake establishes identity, payments establish priority

The separation is important: a client with 1 DOT staked who has paid 100 DOT over time gets better service than a client with 100 DOT staked who has never paid anything. Stake is about identity; payment history is about priority.

---

## Storage Model

Not all data needs the same guarantees. A chat image viewed once and forgotten is different from a family photo backup meant to last decades.

### Write Path

Every write follows the same path:

1. Client uploads data to provider
2. Provider merkleizes and stores chunks
3. Both parties sign a commitment: `{bucket_id: Option, mmr_root, start_seq, leaf_count}`
4. Done - data is stored and commitment is provable

The `bucket_id` is optional:
- **With bucket:** References an on-chain bucket with stake, duration, and slashing terms. Full economic guarantee.
- **Without contract (`None`):** Optional and best-effort storage. Provider serves based on reputation and payment priority. No slashing, but the signed commitment still proves the provider accepted the data.

### Provider Terms

Providers register on-chain with their settings:

- Minimum and maximum agreement duration
- Price per byte per block
- Whether accepting new agreements or extensions

Clients query the chain for authoritative provider settings—this is what the
protocol enforces. Providers also expose a metadata endpoint for dynamic
information that doesn't belong on-chain:

```
GET /info

{
  "status": "healthy",
  "version": "0.1.0",
  "current_load": 0.42,        // 0.0-1.0, helps clients pick least busy provider
  "region": "eu-west",         // hint for latency-based selection
}
```

This is illustrative—the exact fields will evolve. The point: on-chain for
enforceable terms, off-chain for dynamic hints.

Clients choose providers based on:
- Price and duration constraints (from chain)
- Provider stake (from chain—higher stake = more collateral at risk)
- Reputation (past performance, challenge history)
- Dynamic hints (from metadata endpoint)

### Why Storage Agreements Matter

Without agreement: Provider can drop data anytime. Your only recourse is reputation—stop using them, tell others.

With agreement: Provider has stake at risk. You can challenge anytime. Cheating = slashing. The signed commitment + on-chain agreement = economic guarantee.

```
Guarantee Spectrum:
═══════════════════════════════════════════════════════════════════

No bucket, no payment:
  Provider might keep it, might not. Best effort only.

No bucket, with payment:
  Provider prioritizes you. Still no slashing, but reputation matters.

Agreement with low-stake provider:
  Some economic guarantee. Cheap, but slashing hurts less.

Agreement with high-stake provider:
  Strong guarantee. Provider loses significant value if caught cheating.
```

The agreement (or lack thereof) determines what recourse you have if the provider misbehaves.

---

## Buckets and Storage Agreements

A **bucket** is the fundamental unit of storage organization. It defines what
data belongs together, who can access it, and which providers store it.

### Bucket Structure

```
Bucket
├── members: [{ account, role: Admin|Writer|Reader }, ...]
├── frozen_start_seq: Option<u64>     // if set, bucket is append-only
├── min_providers: u32                 // required for checkpoint
└── snapshot: Option<BucketSnapshot>   // canonical MMR state
    ├── mmr_root
    ├── start_seq
    ├── leaf_count
    └── providers: bitfield            // which providers acknowledged
```

**Roles:**
- **Admin**: Can modify members, manage settings, delete data (if not frozen)
- **Writer**: Can append data
- **Reader**: Can read data (for private buckets where providers enforce access)

### Storage Agreements

Each bucket can have storage agreements with multiple providers. An agreement
defines quota, payment, and duration:

```
StorageAgreement
├── max_bytes: u64          // quota for this bucket
├── payment_locked: Balance // prepaid by client
├── expires_at: BlockNumber
└── extensions_blocked: bool
```

**Provider stake is global, not per-agreement.** Providers register with a total
stake amount that covers all their agreements. The Provider struct on-chain
tracks both `stake` and `committed_bytes` (sum of `max_bytes` across all active
agreements). The ratio `stake / committed_bytes` determines trustworthiness. A
provider with 100 DOT stake and 1TB of agreements has a different risk profile
than one with 100 DOT and 100TB of agreements—clients should prefer higher
stake-per-byte ratios for important data.

### Lifecycle

```
1. BUCKET CREATION (on-chain)
   ────────────────────────────────────────────────────────
   • Client creates bucket, becomes Admin
   • Sets min_providers (minimum acknowledgments for checkpoint)
   • Optionally adds members with roles

2. AGREEMENT SETUP (on-chain)
   ────────────────────────────────────────────────────────
   • Client calls request_agreement(bucket, provider, max_bytes, payment, duration)
   • Payment locked from client
   • Provider calls accept_agreement or reject_agreement
   • On acceptance: StorageAgreement created, provider starts tracking usage locally

3. UPLOAD AND COMMIT (off-chain)
   ────────────────────────────────────────────────────────
   • Client uploads chunks to provider
   • Client requests commit → provider signs MMR commitment
   • Data is stored, commitment is provable off-chain

4. CHECKPOINT (on-chain, optional)
   ────────────────────────────────────────────────────────
   • Client submits provider signatures to chain
   • Requires min_providers acknowledgments
   • Establishes canonical state, enables challenge_checkpoint

5. ACTIVE PERIOD (off-chain)
   ────────────────────────────────────────────────────────
   • Provider stores data, serves reads
   • Client can verify randomly, challenge if needed
   • Client can extend_agreement or top_up_agreement

6. SETTLEMENT (on-chain)
   ────────────────────────────────────────────────────────
   • After expires_at: client calls end_agreement with pay/burn decision
   • Or: provider calls claim_expired_agreement after settlement timeout
```

### Binding Agreements

Once accepted, agreements are binding for both parties until expiry:

- **Provider cannot exit early**: They committed to store data for the agreed
  duration. The only way out is to wait for expiry.
- **Client cannot cancel early**: They committed to pay for the agreed duration.
  The locked payment cannot be reclaimed.

**Provider protections:**
- Set `max_duration` in settings to limit exposure
- Call `set_extensions_blocked` to prevent a specific bucket from extending
- Set `accepting_extensions: false` globally to pause all extensions

**Client protections:**
- Challenge if provider loses data → slashing
- At settlement: burn payment to signal poor service

### The Burn Option: Lose - Lose

At agreement end, the client has locked funds that would normally go to the provider. The client can choose to:

1. **Pay** — Provider receives payment. Normal happy path.
2. **Partial burn** — Provider receives less, remainder burned. Signal: "service was poor but functional."
3. **Full burn** — Provider receives nothing, all burned. Signal: "service was unacceptable."

The client loses the funds either way—this isn't about saving money. It's a punishment mechanism for bad service that doesn't rise to the level of a slashable offense (provider did store the data, otherwise they'd be slashed, but barely served it).

**Why burning instead of keeping the funds?**

If clients could reclaim funds by claiming "bad service," they'd have incentive to lie. Burning removes this incentive—you pay either way, you just choose whether the provider benefits.

**Deterrents against abuse:**

- **Reputation loss**: A client who burns loses reputation with that provider. The provider will likely never serve them again, or only at lowest priority.
- **Burn premium** (optional): Burning could cost slightly more than paying—e.g., burn 110% of the contract value. This makes burning a deliberate choice with real cost, not a casual option.

**When to burn:**

Burning is for genuine grievances: the provider technically met their availability commitment (not slashable) but provided really poor read service, slow responses, or other serious quality issues. It's a reputation signal that costs the client nothing extra (beyond losing the provider relationship) but denies the provider revenue they arguably didn't earn.

### On-Chain Challenge

If the provider did not only do a bad service in providing the data, but actually lost it, the client has stronger options than just not paying:  It can force provider to prove data availability on-chain:

```
1. Client initiates challenge
   • Specifies chunks to prove
   • Deposits 100% of estimated challenge cost

2. Provider responds (must respond within deadline)
   • Submits Merkle proofs for challenged chunks
   • Pays tx fee (like any extrinsic)

3. Outcomes:
   
   A. Provider proves successfully:
      • Cost split based on response time (see below)
      • Provider reimbursed from challenger's deposit
      • Challenger gets back their share (remainder after provider reimbursement)
      • Client now has the data (recovered via proof)
   
   B. Provider fails to prove:
      • Provider's stake fully slashed
      • Challenger refunded their deposit + compensation from slash
      • Clear on-chain evidence of fraud
```

**Dynamic cost split based on response time:**

The cost split between challenger and provider shifts based on how quickly the
provider responds. Challenger deposits 100% upfront; after resolution the
provider is reimbursed for tx costs from this deposit, and the split determines
how much the challenger gets back:

- Fast response → challenger pays most (e.g., 90/10), rewarding responsiveness
- Slow response → more balanced (e.g., 50/50), penalizing delay
- Timeout (1-2 days) → full slash, challenger fully refunded + compensated

This incentivizes quick responses without requiring impossibly short deadlines.
The challenge mechanism serves as a **pressure tool**: clients who can't get
data off-chain can recover in a very costly way via challenges, but more importantly forces providers to prioritize their off-chain requests as on-chain response are also more expensive to the provider.

**Why this works:**

- Griefing is expensive: attacker pays 50-90% of challenge cost each time
- Provider stake is untouched by successful challenges (protected from griefing)
- Provider's *conceptual* exposure is still bounded by stake: a rational provider
  stops responding to challenges once cumulative costs approach a significant
  fraction of their stake (e.g., 20-30%), accepting the slash instead. Providers
  can configure this threshold on their nodes.
- Large-scale attacks are visible on-chain; governance can intervene if needed

**Why on-chain resolution is rare:**

Serving data via on-chain proofs costs orders of magnitude more than serving it directly:
- On-chain: gas for proof verification, data availability costs, block space
- Off-chain: bandwidth only

A rational provider always serves directly. The on-chain path exists to make cheating unprofitable, not to be used.

Note: The mechanism mostly exists for challenging and proving that data was lost
to trigger a slash, recovering terabytes of data this way is unrealistic.
Nevertheless the mechanism can be used to recover some data (e.g. the most
important bits) and can also be used to pressure off-chain delivery: E.g. every
time an off-chain request fails, the data is requested on-chain, costing the
misbehaving provider funds. A rational actor would serve the data to avoid the
cost.

### Timing Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Challenge timeout | ~48 hours | Hard deadline, then full slash |
| Settlement window | 3 days | Time for client to call end_agreement after expiry |

**Challenge response cost split:**

The cost split between challenger and provider shifts based on response time,
rewarding fast responses:

| Response time | Challenger pays | Provider pays |
|---------------|-----------------|---------------|
| Block 1 | 90% | 10% |
| Blocks 2-5 | 80% | 20% |
| Blocks 6-20 | 70% | 30% |
| Blocks 21-100 | 60% | 40% |
| Blocks 100+ | 50% | 50% |
| Timeout (~48h) | 0% (refunded) | 100% (slashed) |

This creates strong incentive for immediate response while allowing operational
slack. At 6-second blocks, 100 blocks is 10 minutes—plenty of time for a
well-run provider to respond, but slow enough to penalize negligence.

---

## Read Incentives

Storage guarantees that data *exists*. Read incentives ensure it's *served quickly*.

### The Problem: Storage Proofs Don't Prove Serving

In Filecoin, a provider proves storage via PoSt. But PoSt doesn't prove *serving*. A provider can:
1. Store data correctly (pass all proofs)
2. Refuse to serve it (no penalty from the storage protocol)
3. Collect storage fees anyway

Filecoin addresses this with a separate [retrieval market](https://spec.filecoin.io/systems/filecoin_markets/retrieval_market/)—a deliberate separation of concerns, similar to our approach. Retrieval uses payment channels for off-chain micropayments. More recently, [Project Saturn](https://saturn.tech/) adds a CDN layer with retrieval incentives, and [PDP](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/) (May 2025) enables hot storage optimized for fast retrieval.

### Competition Model

Our approach: make serving data profitable, and let competition drive quality.

From [Proof-of-DOT](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/):

- Providers rank clients in tiers: premium (paid), standard Proof-of-DOT, others
- Priority metric: `DOT_received / requests_served` per client
- Higher ratio = higher priority in serving queue

**The simple model:**

For cheap services (like reads), exact bookkeeping isn't necessary. The feedback is natural:
- Client feels mistreated? Switch providers or stop paying.
- Provider wants to keep revenue? Treat paying clients well.

Clients connect to multiple providers, experience their service quality directly, and vote with their feet (and wallets). No complex negotiation or monitoring APIs required—just "did this work well for me?"

**Future refinement (not required initially):**

Providers could expose metadata APIs: current load, client's priority tier,
payment/request ratio compared to average. Clients could then switch proactively
before quality drops or top up their payment, before service quality degrades.
But this is optimization—the basic model works without it, more optimizations
only make sense once we begin to see serious utilization.

### Why Not Payment Channels?

Filecoin's [retrieval market](https://spec.filecoin.io/systems/filecoin_markets/retrieval_market/) uses payment channels: clients lock funds, providers send data in chunks, clients release payment incrementally via signed vouchers. Fine-grained risk control within a single transfer.

We chose a simpler model, **bandwidth is cheap**:

| Aspect | Payment Channels | Our Model |
|--------|------------------|-----------|
| Trust model | Cryptographic (vouchers) | Reputation (track record) |
| Complexity | Channel lifecycle, vouchers, nonces, deadlines | Simple transfers |
| Risk control | Per-voucher (fine-grained) | Per-prepayment (relationship-based) |
| Overhead | Signing per payment interval | One transfer to top up |

Bandwidth costs ~€0.001/GB. For sub-cent transactions, payment channel overhead (voucher signing, validation, state management) exceeds the value transferred.

Our risk profile: start with small prepayments (~10-30 cents or even less) to less known providers, increase as trust grows. Maximum exposure could be a few euros to your most trusted providers. This is acceptable because providers invest equally in building reputation—there's symmetry in the trust relationship.

### Challenge Economics

The challenge mechanism creates a price ceiling that protects clients even when dealing with a single provider (monopoly on specific data).

**The client's calculus:**

A client who needs data has two options:
1. Pay the provider's asking price
2. Challenge on-chain and recover the data via proof

If the provider demands more than the challenge cost, the rational client challenges. The client pays roughly the same (challenge cost ≈ ransom price), but the provider:
- Loses reputation, forced to deal with on-chain challenge
- Gets no payment for serving
- Loses money, instead of earning

**The provider's calculus:**

A rational provider prices *below* the challenge threshold because:
- Price below threshold → earn payment, client happy, reputation intact
- Price at/above threshold → client challenges, provider loses money instead of earning, reputation damaged

The ceiling is approximately `challenge_cost` per read. Above this, clients prefer the "nuclear option."

**In practice:**

Most of the time, competition drives prices well below this ceiling. The challenge economics only matter for the edge case: a single provider holding unique data and attempting to extract monopoly rents. The ceiling prevents ransom attacks without requiring continuous oversight.

### Hot Content and Viral Distribution

The Proof-of-DOT priority model assumes ongoing relationships: clients pay providers
over time, providers remember and reward loyalty. This works well for most use
cases—backups, application data, private storage.

For viral public content, the dynamic is different:

| Use case | Read:Write ratio | Client relationship |
|----------|------------------|---------------------|
| Backup | 0.01:1 | Long-term, single client |
| Chat media | 10:1 | Moderate, known users |
| Public website | 1000:1 | One-shot, anonymous |
| Viral content | 10000+:1 | One-shot, anonymous |

When millions of anonymous clients each make one request, there's no relationship
to build. The Proof-of-DOT priority system doesn't directly help here.

**How this resolves in practice:**

1. **The bucket owner pays for bandwidth.** Popular content is someone's content.
   They benefit from it being served (reputation, ad revenue, user growth). They
   can prepay providers generously or use multiple providers for capacity.

2. **Free tier with rate limiting.** Providers serve public content at lowest
   priority. When overloaded, paid clients get preference. Anonymous readers
   experience slowdowns but still get served.

3. **Popular providers emerge.** Providers who serve hot content well attract
   more bucket owners who want reliable distribution. This creates some
   centralization—but permissionless centralization. Anyone can compete.

This mirrors how the web works: CDNs exist because serving at scale requires
investment. The difference from Web2: content is verifiable (content-addressed),
so any cache can serve without trust. Clients verify chunks against known hashes
rather than trusting the CDN. And caches can join permissionlessly—no contracts
or business relationships with content owners required.

For high-traffic public content, dedicated cache nodes can provide CDN-like
distribution without storage guarantees. See [Cache Nodes](#cache-nodes) in
Future Directions.

---

## Discovery

How does a client find who can serve content?

### Chain as Entry Point

We recommend using the chain for discovery. The chain provides the root of
trust—from there, you follow references to find data and providers.

The simplest case: a bucket has storage agreements with providers.

```
Bucket → [StorageAgreement(provider_1), StorageAgreement(provider_2), ...]
```

Query the chain for a bucket's storage agreements → get provider accounts →
look up provider multiaddrs → connect directly.

### Hierarchical Discovery

Larger systems compose many buckets. A smart contract or a root bucket serves as
the entry point, with references leading to more specific data:

```
Marketplace Contract
├── references business_1.bucket_id
├── references business_2.bucket_id
└── references catalog.bucket_id
    └── contains data referencing more bucket_ids...

Each business:
└── bucket with storage agreements → providers
```

Discovery follows the reference chain: contract → bucket → data → more buckets.
The chain provides the trusted entry point; the data itself can contain further
references. This allows complex systems where different organizations own
different buckets, all discoverable from a common root.

### Examples

1. **Chat/messaging:** Bucket ID shared in the channel. Look up once, cache
   provider endpoints.
2. **Websites:** Domain → bucket_id mapping (DNS or on-chain). One lookup per
   site.
3. **Backups:** Client created the bucket and agreements. No discovery needed.
4. **Marketplace:** Contract references seller buckets. Buyer discovers sellers
   via the contract, then fetches product data from each seller's bucket.

### Scaling Access

Want content served faster or more reliably? Add more storage agreements:

- Request agreements with additional providers
- Upload data to all providers (client responsibility)
- Clients pick whichever provider has best latency
- Checkpoint with multiple provider signatures for redundancy

More providers = wider access = better availability.

For read-heavy public content, [cache nodes](#cache-nodes) can further extend
reach without requiring storage agreements or upload coordination.

---

## Data Model

### Protocol Layer: Opaque Chunks

At the protocol level, the provider sees only opaque, content-addressed chunks:

```
┌────────────┬────────────┬────────────┬─────────────────┬────────────┐
│  chunk 0   │  chunk 1   │  chunk 2   │      ...        │  chunk N   │
│  (opaque)  │  (opaque)  │  (opaque)  │                 │  (opaque)  │
└────────────┴────────────┴────────────┴─────────────────┴────────────┘
```

Chunks are organized in a Merkle tree. The root hash (`data_root`) commits to all content:

```
                         data_root = H(n0 || n1)
                        /                       \
               n0 = H(h0||h1)                n1 = H(h2||h3)
              /            \                /            \
            h0              h1            h2              h3  ...
            │               │             │               │
         chunk_0         chunk_1       chunk_2         chunk_3
```

**Content addressing is essential:** Each chunk's hash is its identifier. This enables:
- Provability: commitment to data_root commits to exact bytes
- Deduplication: identical chunks share storage
- Integrity: tampering is detectable

**Privacy by design:** The provider stores and serves chunks without knowing what they contain. File structure, names, metadata—all are just (encrypted) bytes to the provider.

### Application Layer: Filesystem

The client controls chunk layout completely. The protocol provides what is essentially a disk: ordered chunks of fixed size, content-addressed for proofs.

Any filesystem technique works:
- Reserved chunks for metadata/directories (e.g., first chunk = root directory)
- Files referenced by byte offset + length
- Inodes, extent trees, FAT—whatever the application needs
- Encryption of all content including directory structure

**Example layout:**

```
Chunk 0-2: [encrypted directory structure - multiple levels fit in large chunks]
Chunk 3-10: [encrypted file: photo1.jpg]
Chunk 11-15: [encrypted file: document.pdf]
...
```

The client reserves the first chunks for directory structure. With large chunk sizes (e.g., 256KB), multiple directory levels fit in a single chunk. The client streams from chunk 0, decrypts directory entries, learns where files live (by byte offset + length), and fetches. The provider sees only "client requested chunks 0, 3-10"—no semantic meaning.

**Trade-offs are application-level:**
- Chunk size (deduplication vs overhead)
- Layout strategy (streaming efficiency vs update cost)
- Fragmentation handling (compaction, free lists, etc.)

### Commitment Model

Each bucket has an associated MMR (Merkle Mountain Range) of data versions. The MMR is per-bucket, not per-provider—all providers storing a bucket should converge to the same MMR state.

**Signed payload:**

```
{
  bucket_id,      // Reference to on-chain bucket, or None for best-effort
  mmr_root,       // root of MMR containing all data_roots
  start_seq,      // sequence number of first leaf in this MMR
  leaf_count,     // number of leaves in this MMR
}
```

**Sequence numbers** are per-bucket, monotonically increasing identifiers for
each MMR leaf (each committed `data_root`). They provide a total ordering of
all versions within a bucket. The `start_seq` indicates where this MMR begins;
combined with `leaf_count`, it defines the range `[start_seq, start_seq + leaf_count)`
of leaves this commitment covers.

When data is deleted, `start_seq` increases (old leaves are pruned). When data
is appended, `leaf_count` increases. This allows efficient reasoning about what
data exists, what was deleted, and what the provider is liable for.

Both client and provider sign this payload. The dual signatures implicitly commit to everything in the MMR.

- **With `bucket_id`:** The commitment is enforceable via on-chain challenge. Provider stake is at risk.
- **Without (`None`):** The commitment proves the provider accepted the data, but there's no on-chain recourse. Reputation-based only.

**MMR leaf structure:**

Each leaf in the MMR contains:

```
{
  data_root,      // Merkle root of content
  data_size,      // size of content under this data_root
  total_size,     // cumulative unique bytes in MMR at this point
}
```

**Why both size fields?**

- `data_size`: The logical size of this specific version's content. Useful for
  clients to understand what each commit contains.

- `total_size`: The cumulative unique bytes stored across the entire MMR history
  at this point. Because chunks are content-addressed, identical chunks across
  versions are deduplicated. This tracks actual storage used—providers use this
  for billing and to enforce quota limits against the storage agreement's
  `max_bytes`.

Example: A 100MB backup where 90MB is unchanged from the previous version has
`data_size = 100MB` but only adds 10MB to `total_size` (the new/changed chunks).
The client is billed for the 10MB increase in `total_size`, not the full 100MB.

The sequence number for any leaf is derived as `start_seq + mmr_position`, so it doesn't need to be stored explicitly. This saves space proportional to MMR size while adding only one field to the signed payload.

By signing the MMR root and start_seq, both parties commit to all leaves and their implicit sequence numbers. One signature covers the entire history.

**Commitment flow:**

1. Alice uploads new data → new `data_root`
2. Provider appends `data_root` as new leaf in MMR
3. Provider computes new `mmr_root`
4. Both sign `{bucket_id, mmr_root, start_seq, leaf_count}`
5. Commitment complete

**On-chain storage is optional:**

The signed commitment is valid off-chain. Alice can:
- Keep it locally (minimal trust assumption)
- Checkpoint on-chain (convenience, discoverability, conflict resolution)

A **checkpoint** is an on-chain transaction that establishes canonical MMR state.
The client submits provider signatures to the chain, proving which providers
acknowledged the state. The chain records the MMR root, sequence range, and
which providers are liable. See the
[implementation doc](./scalable-web3-storage-implementation.md) for the
`checkpoint` extrinsic details.

The bucket on-chain stores terms and optionally the latest checkpointed MMR root. Challenges work with or without on-chain checkpoints—signatures prove validity.

**Implications of optional snapshots:**

Without an on-chain snapshot:
- `challenge_offchain` works (challenger provides provider's signature as proof of commitment)
- `challenge_checkpoint` fails (nothing to challenge)
- `Deleted` defense works (client signature proves deletion authorization)
- `Superseded` defense unavailable (no canonical state to supersede)
- Provider is liable for ALL signed commitments
- Conflicting forks cannot be pruned (no canonical to determine the winner)

With an on-chain snapshot:
- Both challenge types work
- All defenses available
- Conflicts can be pruned once canonical depth exceeds them

Users who create conflicts without checkpointing waste their quota—providers must keep all signed data. Want to clean up? Submit a checkpoint or don't create conflicts to begin with.

**Multi-provider consistency:**

When a bucket is stored by multiple providers, writers are expected to coordinate—sending the same writes to all providers. In the happy path, all providers have identical MMR state.

If providers diverge (network issues, uncoordinated writes), the checkpoint determines which state becomes canonical:

1. Providers accept all uploads within quota—there is no conflict rejection at upload time
2. Different clients can upload different data concurrently without conflicts
3. When a client checkpoints on-chain, that state becomes canonical (recorded as `start_seq` + `leaf_count`)
4. Providers not in the checkpoint can sync by having the client re-upload
5. Non-canonical branches can be pruned once canonical depth exceeds them

**Canonical range and pruning:**

The canonical range is `[start_seq, start_seq + leaf_count)`. A non-canonical branch with range `[A, A+N)` can only be pruned once canonical has range `[B, B+M)` where `B + M > A + N`. This ensures providers remain liable for any data that could still be challenged.

**Challenge defenses based on canonical range:**

- `challenged_seq < canonical.start_seq` → **Deleted** defense (leaf was pruned via deletion)
- `canonical.start_seq <= challenged_seq < canonical_end` → **Superseded** defense (canonical range covers this seq)
- `challenged_seq >= canonical_end` → **No defense** - provider signed something beyond canonical, they're liable. Must respond with **Proof**.

**Superseded defense** covers two cases:
1. **Same data**: The challenged leaf exists in canonical with the same content. Challenger should challenge the canonical snapshot directly.
2. **Forked data**: The challenged leaf was on a conflicting branch that got superseded when a different branch became canonical. Provider rightfully pruned the fork data since canonical won.

In both cases, canonical has "won" for that seq range. The challenger can challenge the canonical snapshot if they believe the data should be there, or accept that their fork lost the conflict resolution.

Note: Off-chain commitments (signed but not checkpointed) can be larger than the on-chain canonical snapshot. Providers are liable for anything they signed that extends beyond canonical. The Superseded defense only works when the challenged state is smaller/older than canonical.

If there is no on-chain snapshot, the `Superseded` defense is unavailable—provider must respond with `Proof` or `Deleted` for any challenge.

**Append-only buckets:**

A bucket can be marked append-only, which:
- Freezes the `start_seq` at the current checkpoint value
- Prevents any future deletions (start_seq can never decrease)
- Is irreversible once set

This ensures historical data cannot be removed, useful for audit logs, public records, or any data that must remain available.

### Challenge Flow

**Challenge from signature** (no checkpoint exists):

1. Alice presents: provider-signed commitment `{bucket_id, mmr_root, start_seq, leaf_count}`
2. Alice specifies: "prove chunk X of leaf at sequence N" (where N is in the committed range)
3. Provider must: provide the `data_root` for leaf N, MMR inclusion proof, and chunk X with Merkle proof to that `data_root`

**Challenge from checkpoint**: Same as above, but Alice only needs to reference
the bucket and provider—the commitment is already on-chain, so no signature
needs to be submitted.

**Deletion defense:**

If data was legitimately deleted (client started fresh MMR):

1. Provider shows: client-signed `{bucket_id, mmr_root_new, start_seq:8, ...}` with higher start_seq
2. The client signature proves the client agreed to the new MMR (which excludes the challenged data)
3. Provider proves: challenged seq is not in new MMR (i.e., `challenged_seq < start_seq`)
4. Challenge rejected

Note: The client signature is essential—without it, a provider could claim data was "deleted" to avoid challenges. The signature proves the client authorized the deletion.

The MMR structure itself proves whether old data still exists. No separate deletion tracking needed.

### Updates and Deletions

**Updates (extending):**

Most updates extend the existing MMR:
- New data_root appended as new leaf
- Old leaves remain valid and challengeable
- Unchanged chunks deduplicated in storage

**Deletions (fresh MMR):**

Client can start a fresh MMR to delete old data:
- New commitment with higher start_seq but MMR containing only new data
- For destructive writes that allow pruning old canonical data: new `start_seq` must be ≥ `old_start_seq + old_leaf_count`
- Old data_roots no longer in MMR → provider can/should delete once canonical depth exceeds old branch
- Challenge against old data_root fails (provider shows `challenged_seq < start_seq`)

Note: Deletions are not allowed on append-only buckets. The `start_seq` is frozen when append-only mode is enabled, and any attempt to checkpoint a different `start_seq` (higher or lower) will be rejected. Only `leaf_count` can increase (appends).

**Size tracking:**

- `data_size`: size of content under the latest leaf's data_root
- `total_size`: cumulative unique bytes across all leaves in MMR

Provider verifies size claims before signing. Deduplication means `total_size` grows slower than sum of `data_size` across versions.

---

## Multi-Provider Redundancy

Storing with a single provider is risky:
- Provider could fail (hardware, bankruptcy)
- Provider could be malicious (collude, ransom data)
- Provider could be slow (poor connectivity to user's region)

### The Collusion Problem

Naive redundancy fails if providers secretly collude:

```
Client stores with providers A and B (thinking: redundancy!)
Reality: A and B are the same entity
Both "fail" simultaneously
Client loses data despite "redundant" storage
```

The main counter measure in this design is the stake, together with picking multiple providers to store the same data, this should already give good guarantees, albeit not perfect: E.g. if they all by accident are the same provider and this one provider is deciding to censor you. Mitigations for this are outside of the scope of this document, but users/applications can build mitigation strategies on top as needed (see below), additionally with proof of personhood we can offer an additional means, with providers optionally proving that they are a person. (Could still collude though)

### Mitigation Strategies
**1. Stake origin analysis**

On-chain, trace where provider stakes originated:
- Fresh stake from exchange: unknown provenance
- Stake from known ecosystem participant: some reputation
- Stake sharing common origin with another provider: suspicious

**2. Historical correlation tracking**

Track failure correlation over time:

```
correlation(A, B) = P(B fails | A fails) / P(B fails)

If correlation >> 1: A and B likely share infrastructure or identity
```

I would expect the error rate to be too low for this to be practical though.

**3. Diversity requirements**

When selecting multiple providers, maximize diversity:
- Different geographic regions
- Different stake origins
- Different ages (time since first contract)
- Different infrastructure (if verifiable)

---

## Encrypted Data: Strong Guarantees Without Sealing

For private/encrypted data, we can achieve Filecoin's uniqueness and incompressibility guarantees *without* slow sealing.

**The scheme (needs cryptographic review):**

```
mask = PRF(provider_key, client_private_key)
stored_data = encrypted_data XOR mask
```

Each provider receives different bytes to store. The provider has `stored_data` and their own `provider_key`, but cannot reconstruct the data without the client's private key.

| Property | Filecoin PoRep | Our Encrypted Scheme |
|----------|----------------|---------------------|
| Unique per provider | Yes | Yes |
| Incompressible | Yes | Yes |
| No cross-provider sharing | Yes | Yes |
| Sealing time | ~1.5 hours | Seconds |
| Unsealing time | ~3 hours | Seconds |
| GPU required | Yes | No |

**Why this works:**

Filecoin needs slow sealing because providers know the sealing algorithm—given enough time, they could regenerate data on demand. Our scheme uses cryptographic secrets instead: the provider *never* knows the mask. No amount of compute helps them reconstruct deleted data.

If a provider loses even one byte, they cannot reconstruct it. A challenge for that byte means full slash. This creates strong incentive for providers to maintain their own redundancy (RAID, backups).

**Limitation:** This only works for private/encrypted data. For public data, anyone can compute the mask. See [Comparison with Existing Solutions](#comparison-with-existing-solutions) for discussion of public data trade-offs.

---

## Use Cases

### Media in Chat

**Scenario:** Group chat where members share photos and videos.

**Requirements:** Low latency, shared access, participants can contribute storage costs, handle concurrent writes.

**Flow:**

```
1. Channel setup (once):
   • Creator creates a bucket on-chain
   • Sets members: admin (creator), writers (participants)
   • Requests storage agreement with 1-2 providers
   • Creator pays initial deposit, sets min_providers=1

2. Adding members:
   • Admin calls set_member to add users with Writer role
   • Members can be added as paying participants (top up agreement)
   • Permissions: Admin can delete, Writers can only append

3. Alice sends a photo:
   • Chunks and encrypts photo with channel key
   • Uploads chunks to provider
   • Commits to bucket's MMR → gets provider signature
   • Sends message to chat: { mmr_root, start_seq, leaf_count, leaf_index }

4. Bob receives message:
   • Looks up bucket on-chain → gets provider endpoint from agreement
   • Requests chunks from provider with MMR proof
   • Verifies against mmr_root from message
   • Decrypts with channel key

5. Concurrent writes (Alice and Carol post simultaneously):
   • Uploads are parallel and conflict-free (content-addressed chunks)
   • Alice uploads photo A chunks, Carol uploads photo B chunks
   • Provider accepts both uploads - data is safely stored
   • Commits require coordination: MMR leaves have an order
   • Chat app establishes message order (e.g., via chat protocol sequencing)
   • Writers commit in agreed order, or one designated committer batches
   • Worst case conflict: data is still there, just re-commit with correct MMR state

6. Checkpoint (occasional, optional):
   • Any member can checkpoint current state on-chain
   • Establishes canonical state for challenge purposes
   • Not required for normal operation - off-chain commits are sufficient
```

**Discovery:** Provider endpoint from storage agreement on-chain. No separate lookup needed.

**Cost sharing:** Members can call `top_up_agreement` to contribute. Agreement tracks total locked payment.

**Latency:** Sub-second for write and read. Commits are off-chain, checkpoints are optional.

**Concurrent writes:** Uploads are parallel and conflict-free. Commits need coordination on order - chat app should establish message sequencing. Worst case conflict: data is safe, just re-commit with correct MMR state.

### Personal Backup

**Scenario:** User backs up ~/Documents (50GB).

**Requirements:** High availability, write-heavy, read-rare (only on recovery).

**Flow:**

```
1. Initial backup:
   • Create a bucket (user is sole Admin)
   • Recursively chunk all files
   • Build directory Merkle tree
   • Encrypt everything with user's master key
   • Select 2-3 diverse providers (check independence)
   • Request storage agreements with each provider
   • Upload to all providers in parallel
   • Commit on each provider → collect signatures
   • Checkpoint on-chain with min_providers=2

2. Incremental backup (next day):
   • Scan for changed files
   • Re-chunk only modified files (content-defined chunking helps)
   • Most chunks unchanged → already exist on providers (deduplication)
   • Upload new chunks, build new directory tree
   • New data_root committed as new MMR leaf
   • Checkpoint periodically (e.g., weekly) for canonical state - or everytime to minimize what the client needs to keep.

3. Verification (ongoing):
   • Periodically pick random chunks from random providers
   • Request with MMR proof
   • Verify against checkpointed mmr_root
   • Challenge on-chain if verification fails

4. Recovery (rare):
   • Fetch directory tree from any provider
   • Distribute chunk downloads across providers in parallel
   • Verify all chunks against MMR proofs
   • Decrypt and reconstruct
   • If one provider fails: others have full copy
```

**Bucket setup:** Single-user bucket, Admin role only, agreements with 2-3 providers.

**Redundancy:** Same data uploaded to multiple providers. min_providers ensures checkpoint requires multiple acknowledgments.

### Public Website / CDN

**Scenario:** Developer deploys static site (500MB of assets).

**Requirements:** Read-heavy, global distribution, low latency.

**Flow:**

```
1. Publish:
   • Create a public bucket
   • Bundle site assets → chunks → Merkle tree → data_root
   • Select multiple providers in different geographic regions
   • Request storage agreements with each provider
   • Upload to all providers in parallel
   • Commit data_root to bucket's MMR on each provider
   • Checkpoint on-chain with min_providers for redundancy
   • Publish DNS: site.example.com → { bucket_id, leaf_index } or just data_root

2. Visitor requests site:
   • DNS → bucket_id + leaf_index (or data_root directly)
   • Look up bucket on-chain → find all provider endpoints
   • Client picks closest/fastest provider (or app suggests based on region)
   • Fetch chunks from selected provider
   • Verify against data_root (self-verifying)

3. Update:
   • Build new site → new data_root
   • Upload to all providers
   • Commit as new MMR leaf on each (leaf_index increments)
   • Checkpoint on-chain
   • Update DNS: site.example.com → { bucket_id, leaf_index: new }
   • Old versions still accessible by old leaf_index (immutable)
```

**Bucket setup:** Public bucket with agreements across multiple geographically distributed providers. Consider append-only (freeze) for audit trail of all versions.

**Global distribution:** Use multiple providers in different regions. Clients fetch from the closest/fastest provider. All providers have the same content (verified by hash).

### Business Backup (Compliance, SLA)

**Scenario:** Enterprise backs up critical data with recovery time guarantees.

**Requirements:** High availability, SLA, audit trail, compliance.

**Flow:**

```
1. Setup:
   • Create bucket with min_providers=3
   • Select 3-5 providers (diversity required, check stake origins)
   • Request storage agreements with each provider
   • Encryption with enterprise key management
   • Freeze bucket immediately for append-only audit trail

2. Backup:
   • Chunk and encrypt data
   • Upload to all providers in parallel (full replication)
   • Each provider commits to same bucket MMR state
   • Checkpoint on-chain (requires min_providers signatures)

3. Monitoring:
   • Continuous verification sampling via off-chain challenges
   • Track response times (SLA compliance)
   • On-chain challenges if off-chain verification fails
   • Alert on failures or SLA violations

4. Recovery:
   • Fetch from any available provider (all have full copy)
   • Parallel fetch across providers for speed
   • If one provider fails, others have identical data
   • SLA violations → on-chain challenge → slashing

5. Compliance:
   • Bucket is frozen (append-only) - immutable audit trail
   • All checkpoints on-chain with timestamps
   • Challenges prove data existed at specific times
   • Provider slashing provides accountability
   • MMR structure proves ordering of all backups
```

**Bucket setup:** Frozen bucket (append-only), min_providers=3, agreements with 3-5 diverse providers.

**Replication:** All providers store identical data. Redundancy through full replication, not erasure coding (would require multiple buckets).

**Audit trail:** Frozen bucket ensures no deletion. On-chain checkpoints provide timestamped proof of all data versions.

---

## Comparison with Existing Solutions

| Aspect | This Design | Filecoin | Arweave | Celestia/Avail |
|--------|-------------|----------|---------|----------------|
| **Primary focus** | General storage + reads | Provable storage | Permanent storage | Data availability |
| **Proof mechanism** | Game-theoretic (challenges) | Cryptographic (PoRep/PoSt) | Cryptographic (PoA) | Cryptographic (DAS) |
| **Chain load** | Minimal (commits + disputes) | Heavy (continuous proofs) | Moderate | Moderate |
| **Read incentives** | Proof-of-DOT priority | Retrieval market + Saturn | Endowment model | N/A |
| **Write latency** | Sub-second | Minutes-hours | ~2 minutes | Seconds |
| **Trust model** | Economic (slash on fraud) | Cryptographic | Cryptographic | Cryptographic |
| **Storage duration** | Contract-based (flexible) | Deal-based (fixed) | Permanent | Short-term (DA only) |
| **Data delivery enforcement** | On-chain (expensive fallback) | None | None | N/A |

### Continuous vs On-Demand Proving: A Deeper Comparison

Most decentralized storage systems use continuous or periodic proving. We use
on-demand challenges. This is a deliberate trade-off:

| Aspect | Continuous Proving (Filecoin, StorageHub) | On-Demand Challenges (This Design) |
|--------|-------------------------------------------|-----------------------------------|
| **Detection guarantee** | 100% per period | Probabilistic (depends on client checks) |
| **Chain load** | O(storage volume) | O(disputes) ≈ 0 in happy path |
| **Abandoned data** | Still proven | No guarantee without active client |
| **Hardware requirements** | Often specialized (GPU for sealing) | Commodity hardware |
| **L2 required for scale** | Yes (Filecoin acknowledges this for PDP) | No |
| **Verifier's dilemma** | Avoided (protocol challenges) | Avoided (verification: normal use/cheap) |
| **Security model** | Cryptographic | Economic/game-theoretic |

**Why continuous proving exists:**

Academic literature identifies real attacks that continuous proving prevents:

1. **Timing attacks:** If challenge response window > time to regenerate data,
   providers can delete and regenerate on demand. Filecoin solves this with
   slow sealing (~1.5h) and fast deadlines (30min).

2. **The verifier's dilemma:** If verification costs effort and most providers
   are honest, rational clients won't verify, degrading security over time.

**Why we don't need it:**

1. **No sealing = no timing attack.** We don't rely on computational barriers.
   Instead, stake at risk makes cheating unprofitable regardless of timing.

2. **Verification has no real cost.** Bandwidth is flat-rate for most clients
   today. Client software simply checks a few random chunks by default whenever
   it runs—no user effort, no marginal cost, just good defaults. The verifier's
   dilemma assumes verification is costly; when it's free, the dilemma
   disappears.

3. **Someone must care.** We assume client software is at least occasionally
   started. If that's not the case—data that must persist without any client
   ever running again—Filecoin's continuous proving is the better solution.

### Data Delivery: A Key Differentiator

Proving storage is not the same as serving data.

In Filecoin, PoSt proves a provider *has* data. But nothing forces them to *serve* it at the storage protocol level. The retrieval market handles serving incentives separately—providers earn fees for retrieval, and Project Saturn adds a CDN layer. However, if a provider refuses to serve, the client's recourse is finding another provider or using a different retrieval path. For unique data stored with only one provider, options may be limited.

Our design has an enforcement mechanism:

```
Provider refuses to serve data
    ↓
Client initiates on-chain challenge
    ↓
Provider MUST submit chunk data in proofs (not just hashes)
    ↓
Data is now on-chain (expensive but recovered)
    ↓
If provider fails: full slash + client gets compensation
```

The on-chain path is expensive—submitting Merkle proofs with actual chunk data costs significant gas and is slow/has limited bandwidth. This makes recovery of large data unrealistic, but it puts economic pressure on providers, which makes the cheap direct-serving path incentive-compatible.

A rational provider always serves directly because:
- Direct serving: earn payment, bandwidth cost only
- Being challenged: pay provider fraction of challenge cost, operational burden, lose reputation, data served anyway via chain

The expensive on-chain path is the "nuclear option" that prevents ransom attacks. It exists to never be used.

### Saturn and Storacha: Filecoin's Hot Storage Layer

[Saturn](https://saturn.tech/) and [Storacha](https://storacha.network/) represent Filecoin's evolution toward hot storage—similar goals to this design.

| Aspect | Saturn/Storacha | This Design |
|--------|-----------------|-------------|
| Sybil resistance | Centralized orchestrator | Proof-of-DOT |
| Payment model | Pooled monthly payouts | Direct client→provider |
| Node membership | Orchestrator approval | Permissionless with stake |
| Storage proofs | PDP (periodic) | Challenges (on-demand) |
| Delivery enforcement | Reputation + testing | On-chain challenges with slashing |

The key difference is enforcement: Saturn/Storacha rely on reputation and testing (SPARK). Our design has on-chain challenges as a fallback—expensive, but a credible threat that makes off-chain cooperation incentive-compatible.

### Polkadot Ecosystem: Eiger and StorageHub

Two teams are building storage solutions for Polkadot. Understanding their approaches clarifies our design choices.

**Eiger: Polkadot Native Storage**

Eiger is porting Filecoin's traditional architecture to Polkadot:

- Full PoRep/PoSt implementation (sealing, 32GB sectors, continuous proofs)
- Requires GPU for proof generation (CUDA/OpenCL)
- Porting `rust-fil-proofs` and Filecoin's `builtin-actors`
- WindowPoSt every 24 hours per sector

This is the "old" Filecoin model—strong cryptographic guarantees but heavy hardware requirements and continuous chain load. It is also unsuitable for most interactive use cases: Latency is high and a 32GB sector size also makes this model impractical for many applications.

**StorageHub (Moonsong Labs)**

StorageHub takes a simpler approach with continuous random challenges:

```rust
// From their proofs-dealer pallet
type RandomChallengesPerBlock: Get<u32>;  // Default: 10 per block
```

- Merkle proofs (like ours), no sealing
- But: challenges every block, not on-demand
- Providers must respond within `ChallengePeriod` or get slashed
- Explicitly rejected PoRep "because it demands expensive hardware"

Their trade-off: simpler than Filecoin, but still continuous on-chain verification.

**Comparison**

| Aspect | Eiger | StorageHub | This Design |
|--------|-------|------------|-------------|
| Proof model | PoRep/PoSt | Continuous Merkle | Game-theoretic |
| Challenges | Every 24h (PoSt) | Every block | On-demand only |
| Chain load | Heavy | Heavy | Minimal |
| Hardware | GPU required | Modest | Modest |
| 32GB sectors | Yes | No | No |
| Sealing | ~1.5 hours | None | None |

**Key insight:** Both use *continuous verification*—the chain is constantly busy with proofs. We use *deterrent-based verification*—the chain is quiet unless someone cheats.

The game-theoretic literature supports this: "infrequent but perfectly verifiable audits are sufficient to enforce truthful behavior." A rational provider stores data correctly because they *could* be challenged at any time, not because they *are* challenged continuously.

### Filecoin's Two-Tier Future: PoRep + PDP

Filecoin launched [Proof of Data Possession (PDP)](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/) on mainnet in May 2025—a hot storage layer alongside cold storage (PoRep).

| Property | PoRep (cold) | PDP (hot) |
|----------|-------|-----|
| Unique copy proven | Yes (sealed) | No |
| Immediate access | No (~3hr unseal) | Yes |
| Mutable | No | Yes |

We're designing for the *hot storage* use case. Filecoin's PDP validates that there's demand for this. But we differ in two ways:

1. **Challenge frequency:** PDP still requires ongoing proofs. We use rare, on-demand challenges.
2. **Data delivery enforcement:** PDP proves possession but doesn't force serving. Our challenges require submitting actual data on-chain.

Our approach: zero chain load in the happy path.

### What We Give Up: The Game-Theoretic Trade-off

Our design bets on rational adversaries. This is a weaker security model than continuous cryptographic proofs. What could go wrong?

**Attack: The Lazy Provider**

A provider accepts data, commits the Merkle root, then deletes most of it—betting that:
1. Most clients never challenge
2. The few who challenge can be paid off or ignored
3. Expected profit from cheating > expected loss from slashing

This attack is *rational* if `P(challenge) × slash_amount < storage_cost_saved`.

**Our mitigations:**

1. **Challenge cost asymmetry.** Challenger pays 50-90% of challenge cost depending on response time. If provider fails to respond, they lose their *entire stake*. Expected loss for cheating = `P(challenge) × full_stake`, which dominates storage savings for any reasonable challenge probability. Rational clients will prefer higher-staked providers.

2. **Reputation visibility.** Failed challenges are on-chain. A provider with even one failed challenge is radioactive—no rational client stores with them. The reputational loss exceeds any single-contract gain.

**Attack: The Malicious Challenger**

A competitor or griefer challenges a honest provider repeatedly to drain their funds or force them offline.

**Our mitigations:**

1. **Challenger pays most.** Challenger loses 50-90% of their deposit on each successful challenge, making sustained griefing expensive.
2. **Provider stake untouched.** Successful challenges don't deplete provider stake—it's protected from griefing attacks.
3. **Rational exit threshold.** Provider's conceptual exposure is bounded: once cumulative challenge costs approach a significant fraction of stake (e.g., 20-30%), a rational provider stops responding and accepts the slash rather than continue the costly game.

**Attack: Data Withholding After Commitment**

Provider commits root, client pays, provider refuses to serve—betting client won't pay challenge cost.

**Our mitigations:**

1. **Challenge recovers data.** Unlike Filecoin, our challenge *forces* data on-chain. Provider can't just prove possession—they must submit the actual chunks.
2. **Challenge cost < ransom cost.** If provider demands ransom > challenge cost, rational client challenges.
3. **No profit in forcing challenges.** Provider gains nothing from making clients challenge—they just deal with operational overhead and reputation damage and pose a share of the cost.

**What we can't prevent:**

- **Irrational adversaries.** Someone willing to lose money to hurt others. Mitigated by stake requirements (expensive to be irrational at scale).
- **Lazy clients.** If no one ever challenges, the deterrent weakens. Mitigated by making challenges profitable when they catch cheaters, making them default (built into client software) and cheap/free - if data is served off chain, on chain challenge is unnecessary.

**Design assumption: Someone must care.**

Storage guarantees are only meaningful when someone has an active interest in
the data. A rational client who paid for storage will periodically verify their
data exists—either by reading it for actual use, or by automatic background
checks built into client software.

If no one ever reads or checks data, that data has no practical value worth
guaranteeing. For archival use cases where data must persist without active
verification ("store once, never check again"), we recommend purpose-built
systems like Filecoin's cold storage tier.

This is a feature, not a limitation: we optimize for the common case (active
data with interested parties) rather than paying continuous overhead for the
edge case (abandoned data).

**The honest assessment:**

Continuous proofs (Filecoin PoSt, StorageHub) catch *every* cheater, every time. We catch cheaters *probabilistically*, when challenged. This is weaker. We accept this trade-off because:

1. Chain load scales with storage volume in continuous systems. Ours scales with disputes (rare).
2. Hardware requirements are lower (no GPU, no sealing).
3. The game theory is sound: rational providers don't cheat when cheating has negative expected value.

For archival storage where "never lose a bit" matters more than cost, use Filecoin with PoRep. For hot storage where performance and cost matter, the game-theoretic model is sufficient.

### Public Data: The Funding Problem

For private data, our [encrypted storage scheme](#encrypted-data-strong-guarantees-without-sealing) provides PoRep-equivalent guarantees without sealing. But for public data, anyone can compute the mask—the scheme doesn't apply.

However, PoRep for public data doesn't solve the real problem either: **who pays for redundant copies?**

| Data State | What Happens |
|------------|--------------|
| Popular/valuable | Many nodes cache it (serving is profitable) |
| Unpopular | No profit in serving—who pays? |

Filecoin's PoRep can *prove* that N unique copies exist, but someone still has to fund each storage deal. The options are the same in both systems: altruism, collective funding (DAO/foundation), or the uploader pays.

**Our position:** Public cold storage with proven redundancy is not our primary objective. We optimize for hot storage. For public archival data, use Filecoin—it doesn't matter where archival data sits as long as it's preserved.

### When to Use What

**This design:** You need storage with strong read performance. Duration is variable. Cost matters. You're building an application that interacts with storage frequently.

**Filecoin:** You need provable archival storage. Read performance doesn't matter. You're storing cold data for years.

**Arweave:** You need permanent storage. Pay once, forget forever. Censorship resistance is critical.

**Celestia/Avail:** You need data availability for rollups. Short-term (weeks), high throughput, don't need general storage.

---

## Related Work

**Filecoin** ([docs](https://docs.filecoin.io/storage-providers/filecoin-economics/storage-proving)) uses Proof-of-Replication and Proof-of-Spacetime. Every 24 hours, every sector must submit proofs. Strong cryptographic guarantees, but significant chain load. Read incentives are handled separately via the retrieval market, payment channels, and more recently Project Saturn (CDN layer) and PDP (hot storage).

**"Rationally Analyzing Shelby" (2025)** ([arXiv:2510.11866](https://arxiv.org/abs/2510.11866)) provides formal game-theoretic analysis of storage incentives. Key insight: "infrequent but perfectly verifiable audits are sufficient to enforce truthful auditing off-chain." This validates our challenge-based approach—the ability to challenge is sufficient; continuous proofs are unnecessary.

**IPNI** ([docs](https://docs.ipfs.tech/concepts/ipni/)) provides centralized indexing for IPFS, achieving ~500µs lookups vs. seconds for DHT. Demonstrates that hybrid approaches (fast centralized + decentralized fallback) are practical.

**Optimistic Provide (2024)** ([IEEE](https://ieeexplore.ieee.org/document/10621404/)) reduces IPFS DHT publish latency from 10-20s to sub-second for 90% of operations. Shows that DHT performance can be dramatically improved with heuristics.

---

## Bootstrapping and Rollout

A common concern: how does a decentralized network function before critical mass?

The key insight: providing the service is cheap. Bandwidth costs are sub-cent per GB. Unused infrastructure capacity costs money whether used or not. This means providers can offer free service while building reputation—the marginal cost of serving free-tier users is negligible compared to the reputation value gained.

**Rollout phases:**

**Phase 1: Buckets and Basic Storage**

Deploy on-chain bucket infrastructure and storage agreements. Parity/ecosystem
runs initial providers offering free or low-cost storage. This establishes:
- On-chain discovery (buckets → agreements → provider endpoints)
- Working protocol and implementation
- Initial user base building applications

**Phase 2: Challenges and Guarantees**

Add the challenge mechanism for storage agreements. Providers must lock stake.
Clients can challenge if data is unavailable. This establishes:
- Economic guarantees beyond reputation
- Slashing for misbehavior
- Trust model for critical data

**Phase 3: Introduce Proof-of-DOT**

Add Proof-of-DOT for sybil resistance and read prioritization. Users who
register get priority; providers track payment history. This establishes:
- Identity layer
- Quality-of-service differentiation for reads
- Foundation for competitive provider market

**Phase 4: Third-Party Providers and Payments**

Open the system to third-party providers competing on price and quality.
Payment tracking enables sustainable economics. This establishes:
- Economic sustainability
- Provider competition
- Market-driven pricing

**Why this works:**

Each phase is functional on its own. There's no circular dependency (need users to attract providers, need providers for users). The system works at every stage—it just gets better as it evolves.

Providers who join early build reputation before competition intensifies. Users who join early get free/cheap service while the market develops. Everyone has incentive to participate at every phase.

---

## Operational Considerations

This section provides concrete guidance for implementers and operators.

### Provider Economics

Storage providers operate on thin margins. The value proposition is not outsized
profits but rather:

- **Decentralization premium.** Users pay slightly more than centralized
  alternatives for censorship resistance and redundancy guarantees.
- **Ecosystem integration.** Providers who already run validators or collators
  can amortize infrastructure costs across services.
- **Market positioning.** Early providers build reputation before competition
  intensifies.

Providers should expect margins comparable to traditional cloud storage (~10-20%
above infrastructure costs) rather than cryptocurrency-style returns.

### Recommended Parameters

These are starting points, tunable via governance:

| Parameter | Recommended Value | Rationale |
|-----------|-------------------|-----------|
| Chunk size | 256 KB | Balances proof size vs overhead |
| Min stake per GB | 70-100 μDOT/GB | Must deter lazy providers (see below) |
| Min agreement duration | 1 week | Prevents gaming via short commitments |
| Max agreement duration | 2 years | Limits long-term stake lockup |
| Challenge deposit | ~Cost of 256KB on-chain | Must cover chunk submission in response |
| Challenge timeout | ~48 hours (blocks) | Allows for temporary outages |

**Stake rationale:**

Cheating is nearly impossible to hide. From PoR (Proof of Retrievability)
literature, if a provider deletes fraction `f` of data and a client samples `n`
random chunks:

```
P(caught) = 1 - (1-f)^n
```

| Fraction deleted | 10 samples | 100 samples | 365 samples (daily) |
|------------------|------------|-------------|---------------------|
| 1% | 9.6% | 63% | 97% |
| 10% | 65% | 99.99% | ~100% |
| 50% | 99.9% | ~100% | ~100% |

Daily sampling is trivial for any client (phones, IoT, apps). Some clients
periodically download all their data (backups, restores)—100% detection.

**Passive verification in practice:**

Client applications should verify storage as a background operation during
normal use. A backup client that runs weekly can check a few random chunks after
each sync—trivial bandwidth cost (~1MB), zero user effort. Consider a typical
backup app:

```
Backup app runs weekly
  → Upload new chunks
  → Fetch 3 random old chunks (trivial bandwidth, ~768KB)
  → Verify against known root
  → Done
```

With weekly checks of 3 chunks, detection probability over time:

| Provider deleted | 1 month | 3 months | 6 months | 1 year |
|------------------|---------|----------|----------|--------|
| 1% of data | 20% | 49% | 74% | 93% |
| 5% of data | 65% | 96% | 99.8% | ~100% |
| 10% of data | 88% | 99.8% | ~100% | ~100% |

This eliminates the "verifier's dilemma" identified in academic literature:
verification isn't a costly conscious decision, it's an automatic byproduct of
using the system. The user doesn't even know it's happening.

**Why stake per GB?**

When caught, the provider loses their *entire* stake—not just the stake
proportional to that agreement or the deleted data. A single failed challenge
means losing everything. Even deleting 1% of one client's 10TB saves ~$0.12/year
in storage costs while risking the full stake across all agreements. Any
meaningful stake makes cheating absurd economics.

The stake-per-GB ratio ensures **pain scales with provider size**:
- Small provider (10TB) with 100 DOT at stake: losing it hurts
- Large provider (1PB) with 100 DOT at stake: losing it is a rounding error

The ratio keeps incentives aligned regardless of operation size.

**Example stakes:**

| Tier | Stake ratio | 10TB provider | Use case |
|------|-------------|---------------|----------|
| Baseline | 5-10 μDOT/GB | 50-100 DOT ($350-700) | General storage |
| Standard | 10-20 μDOT/GB | 100-200 DOT ($700-1,400) | Business data |
| Gold | 50-100 μDOT/GB | 500-1,000 DOT ($3,500-7,000) | Critical/enterprise |

**Users control their risk:**
- **Sample frequently** — daily checks catch almost everything
- **Use multiple providers** — one losing data doesn't mean total loss
- **Choose stake tier** — match provider commitment to data importance

**Challenge deposit calculation:** The deposit must cover the cost of submitting
a full chunk on-chain (provider's response includes the challenged chunk with
Merkle proofs). With 256KB chunks, this is the dominant cost. The exact amount
depends on chain weight pricing and should be calculated dynamically or set via
governance based on current rates.

### Garbage Collection

Providers must implement garbage collection for chunks with no active
agreements. Recommended approach:

1. Track reference counts per chunk (how many active agreements reference it)
2. When count reaches zero, mark for deletion after optional grace period (e.g., 7 days)
3. Grace period allows for agreement renewals without re-upload

Clients should not assume chunks persist beyond their agreement duration.

---

## Future Directions

### Cache Nodes

A dedicated cache node type could extend the provider model for content delivery:

**Key differences from providers:**
- No stake requirement, no challenges, no availability guarantees
- Can self-add to buckets they find profitable (predict viral content)
- Responsible for fetching data they want to cache from actual providers
- Serve reads for payment, compete on latency and regional presence

**Discovery is trivial:** Caches are bucket members. Clients query the bucket,
get the list of providers and caches, try them based on region/reputation.

**Quality enforcement via Proof-of-DOT:** Bad cache? Drop it, try another. Good
cache hitting rate limits? Start paying. No complex on-chain verification
needed—market dynamics handle quality.

**Economics for content providers:**
- Option A: Pay caches to join your bucket (hard to verify they're serving)
- Option B: Let the market work—caches predict demand and join speculatively
- Option C: Similarly to (A), operators just add publicly available caches to
  their buckets - pay the chain costs. Delivery costs are covered by users.

Option B is cleaner. Caches are profit-seeking businesses that bear prediction
risk. Popular content attracts caches naturally; unpopular content doesn't need
them. Option C might also work quite well.

---

## Summary

The design achieves scalability by removing the chain from the hot path:

| Operation | Chain Interaction | Frequency |
|-----------|-------------------|-----------|
| Write | None | High |
| Read | None | High |
| Bucket/agreement setup | One tx each | Low |
| Checkpoint | Optional, batched | Low |
| Challenge | Only if fraud | Rare |

**Key design choices:**

1. **Buckets as organizational unit.** A bucket defines what data belongs
   together, who can access it (members with roles), and which providers store
   it (via storage agreements). Buckets can be frozen for append-only semantics.

2. **Storage agreements.** Per-bucket, per-provider contracts with quota,
   duration, and prepaid payment. Binding once accepted—neither party can exit
   early. Providers lock stake proportional to committed bytes.

3. **Unified write model.** Every write follows the same path: upload,
   merkleize, commit. The `bucket_id` in the commitment is optional—with a
   bucket you get slashing guarantees, without you get best-effort storage.

4. **Protocol-layer opacity.** Providers see only content-addressed chunks. File
   structure, metadata, directories—all application-layer concerns. Privacy by
   design: encrypt everything, provider learns nothing.

5. **MMR-based commitments.** Per-bucket Merkle Mountain Ranges track version
   history. Provider signs `{bucket_id, mmr_root, start_seq, leaf_count}`.
   Canonical range is `[start_seq, start_seq + leaf_count)`. Deletions via fresh
   MMR with higher start_seq (requires client signature).

6. **Proof-of-DOT foundation.** Sybil resistance via DOT staking enables
   identity and reputation. Payment history (not stake) determines service
   priority.

7. **Game-theoretic enforcement.** Challenges replace continuous proofs.
   Rational providers serve data because being challenged is expensive. The burn
   option lets clients punish bad service at their own cost.

8. **Bucket-based discovery.** Providers are found via bucket storage
   agreements. For peer-to-peer sharing, bucket ID travels with the content
   reference.

The chain exists as a credible threat. Rational actors never use it.

---

## Appendix: Detailed Comparisons

This appendix provides deeper technical comparisons for readers evaluating alternatives.

### A.1 Saturn and Storacha Deep Dive

[Saturn](https://saturn.tech/) and [Storacha](https://storacha.network/) represent Filecoin's evolution toward hot storage and fast retrieval.

**Saturn** is a CDN layer:
- L1 nodes (edge caches in data centers) and L2 nodes (home computers)
- Node operators earn FIL based on bytes served, speed metrics, and uptime
- Centralized orchestrator manages node membership and payment distribution
- Monthly payouts via FVM smart contract
- Minimum 14 days uptime per month to qualify for earnings

**Storacha** combines web3.storage with Saturn:
- Three node types: storage (persist data), indexing (track location), retrieval (CDN/cache)
- Uses UCANs for authorization
- Backed by Filecoin PDP for storage proofs
- Moving to L2 using IPC (Interplanetary Consensus) for scalability

Saturn's centralized orchestrator is a pragmatic choice for bootstrapping—it manages quality control and payment distribution. Our design is more decentralized from the start, using Proof-of-DOT for permissionless participation and on-chain mechanisms for enforcement.

Storacha separates storage/indexing/retrieval into distinct node roles. Our design allows any provider to serve all roles, with specialization emerging from economics rather than protocol requirements.

### A.2 Why Filecoin Needs PoRep (And We Don't)

PoRep's sealing creates an *incompressible, unique* encoding of the data. This is essential for storage-based consensus:

1. A provider claims "I'm storing 32GB"
2. PoRep proves they're storing 32GB of *unique* data that *cannot be regenerated on demand*
3. The sealing is slow enough (~1.5 hours) that providers can't fake it during the 24-hour PoSt window

Without this, a malicious provider could:
- Store one copy, claim ten
- Regenerate data from a compressed version when challenged
- Share storage across "independent" providers

PDP only proves "I can access this data"—not "I'm dedicating unique physical storage to it." That's fine for hot storage (you just want the data served fast), but breaks the security model that underpins Filecoin's consensus.

**Filecoin's architecture now:**

```
┌─────────────────────────────────────────────┐
│              Filecoin Network               │
├─────────────────────────────────────────────┤
│   Cold Storage (PoRep + PoSt)               │
│   • Archival, long-term                     │
│   • Sealed sectors, 32GB                    │
│   • Consensus participation                 │
│   • ~3 hour unsealing for retrieval         │
├─────────────────────────────────────────────┤
│   Hot Storage (PDP)                         │
│   • Fast retrieval                          │
│   • No sealing, raw data                    │
│   • Mutable collections                     │
│   • No consensus weight                     │
└─────────────────────────────────────────────┘
```

### A.3 Payment Channels: Full Comparison

Filecoin's [retrieval market](https://spec.filecoin.io/systems/filecoin_markets/retrieval_market/) uses [payment channels](https://spec.filecoin.io/systems/filecoin_token/payment_channels/) with incremental payments:
- Client creates payment channel on-chain, locks funds for entire retrieval upfront
- Provider sends data in chunks, pauses when payment needed
- Client sends signed vouchers (cumulative value, increasing nonce) as data arrives
- Provider submits final voucher on-chain within 12hr window to collect
- Vouchers are channel-specific (client → provider), so no double-spend risk

Payment channels let you control risk at arbitrary granularity within a single transfer—you release payment incrementally as data arrives. If the provider stops, you've only authorized payment for what you received so far. Note: this is pure risk control, not dispute resolution. Vouchers prove what you *signed*, not what you *received*. Neither system can prove delivery.

**Implementation complexity:**

Payment channels require:
- On-chain channel open/close transactions
- Channel state machine (open, active, settling, closed)
- Voucher signing, validation, and persistent storage
- Nonce tracking to handle out-of-order vouchers
- Settlement deadline handling (Filecoin uses 12hr windows—miss it and funds revert)
- Per-provider channel management (funds locked in channel A can't pay provider B)

Our model requires:
- On-chain balance transfers (already exists)
- Provider tracks `received/served` per client (simple map, recoverable from chain if lost)
- Client tracks quality stats per provider (local only)

**User experience:**

Payment channels require users to:
- Decide upfront how much to lock per provider
- Manage channel lifecycles (open, close, reopen for different providers)
- Ensure funds are in the right channel before requesting service
- Monitor settlement windows (or risk losing unclaimed funds)

Our model: transfer some DOT to a provider when you want priority. Done. Switch providers freely—no channel management, no locked funds.

**Why not payment channels for storage contracts?**

One might consider payment channels for long-term storage: client releases payments incrementally over the contract lifetime, stops paying if problems arise. But this creates new problems:

- Client must come online regularly to release payments
- If client disappears, provider faces a dilemma: delete data (breaking availability) or continue serving unpaid (hoping for late payment)
- Long lockup periods tie up client funds
- The "store and forget" use case breaks entirely

No reusable payment channel library exists for Substrate anyway—Filecoin's [go-fil-markets](https://github.com/filecoin-project/go-fil-markets) is Go and tightly coupled to Filecoin, [Rust-Lightning (LDK)](https://github.com/lightningdevkit/rust-lightning) is Bitcoin-specific. Building from scratch isn't justified when the simple model covers our use cases.
