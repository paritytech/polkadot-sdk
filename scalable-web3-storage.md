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
6. [Storage Model](#storage-model)
7. [Availability Contracts](#availability-contracts)
8. [Read Incentives](#read-incentives)
9. [Discovery](#discovery)
10. [Data Model](#data-model)
11. [Multi-Provider Redundancy](#multi-provider-redundancy)
12. [Use Cases](#use-cases)
13. [Comparison with Existing Solutions](#comparison-with-existing-solutions)
14. [Bootstrapping](#bootstrapping)
15. [Open Questions](#open-questions)

---

## Introduction

Existing decentralized storage systems put the blockchain in the hot path. Filecoin requires continuous on-chain proofs every 24 hours for every sector. StorageHub issues random challenges every block. This creates fundamental scalability limits: chain throughput bounds storage capacity.

We propose a different architecture: **the chain exists as a credible threat, not as the happy path**. In normal operation, clients and providers interact directly—no chain transactions for writes, reads, or ongoing storage. The chain is only touched for identity registration (once), payments to providers (simple transfers for priority service), availability commitments (batched, infrequent), and dispute resolution (rare, expensive, avoided by rational actors).

This works because of two foundations:

**Proof-of-DOT** provides sybil resistance and enables reputation. Clients lock DOT against a PeerID; providers track payment history per client. Service quality becomes a competition: providers who serve well attract paying clients, providers who don't get dropped. No complex payment channels needed—just simple transfers and local bookkeeping. The amounts are small (bandwidth costs ~€0.001/GB), so fine-grained payment accounting isn't worth the overhead.

**Game-theoretic enforcement** replaces continuous cryptographic proofs. Providers commit Merkle roots on-chain and lock stake. Clients can challenge at any time, forcing the provider to prove data availability or lose their stake. The challenge mechanism is expensive for everyone—which is the point. Rational providers serve data directly because being challenged costs them money even if they're honest. Rational clients don't challenge frivolously because it costs them too. The expensive on-chain path exists to make the cheap off-chain path incentive-compatible.

The result: storage that scales with provider capacity, not chain throughput. Writes are instant (no consensus). Reads are fast (direct from provider, no DHT lookup required). Guarantees are optional and tiered—ephemeral data needs no on-chain commitment, critical backups get full availability contracts with slashing.

This document details the design: tiered storage model, availability contracts, read incentives, discovery, and how it compares to existing solutions.

---

## Status Quo

### The Write Problem

Filecoin was designed around sealed cold storage. The traditional path requires sealing data into sectors with cryptographic proofs—sealing a 32GB sector takes ~1.5 hours and requires specialized hardware (GPUs, CPUs with SHA extensions). 

Filecoin has evolved: [Proof of Data Possession (PDP)](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/), launched on mainnet in May 2025, provides a simpler alternative for hot storage without sealing. This moves in a similar direction to the design proposed here. PDP is a legitimate alternative worth considering, though it still requires periodic on-chain proofs and the complexity of Filecoin's deal infrastructure.

IPFS allows fast writes (just send data to a node), but provides no guarantee the data will persist. The node can drop it immediately or refuse to accept it.

### The Storage Problem

Proving that data is stored is expensive. Filecoin uses Proof-of-Replication (PoRep) and Proof-of-Spacetime (PoSt), requiring continuous on-chain proofs. Every 24 hours, every sector must be proven. This creates enormous chain load and operational complexity for storage providers.

IPFS requires no proofs at all, meaning users have no assurance their data still exists, which becomes visible in the known poor user experience of IPFS.

### The Read Problem

This is where existing systems fail most dramatically. Consider the data flow:

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

1. **Write without consensus.** Data writes should not touch the chain. A user sending an image in a chat should experience sub-second latency with no on-chain transaction.

2. **Optional, efficient storage guarantees.** Not all data needs the same guarantees. Ephemeral chat images don't need on-chain contracts. Critical backups do. The system should support both, with chain interaction only when guarantees are requested.

3. **Incentivized reads.** Providers must be economically motivated to serve data quickly, not just prove they have it. Read performance should improve with competition, not degrade with scale.

4. **Accountable providers.** When a provider commits to storing data, that commitment must be enforceable. Cheating must be detectable and punishable without requiring continuous on-chain proofs.

5. **Permissionless participation.** Anyone can become a storage provider or cache. No gatekeepers, no special hardware requirements beyond storage capacity.

---

## Non-Goals

**Database-style access.** This design optimizes for file-like patterns: store blobs, retrieve by content hash, read ranges. Not for small random key-value lookups, indexes, or queries. You *can* build a Merkle trie on top (like blockchain state), but then you've built a blockchain's state layer—at which point, use a blockchain. Web3 databases are inherently slower than Web2 databases due to consensus; that's a fundamental tradeoff, not something storage-layer design can fix.

**Permanent storage.** Unlike Arweave, we do not aim for "store once, available forever." Storage has a duration, contracts have terms. This is a feature: it allows for deletion, reduces costs, and matches how most applications actually use storage.

**Privacy at the protocol level.** Private data is encrypted data. The storage layer sees opaque bytes. Key management, access control, and encryption are application-layer concerns. Future versions could add access control hints (e.g., "only serve to these credentials"), but enforcement ultimately relies on encryption—a provider can always be curious. For now, we keep it simple: bytes are bytes.

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
│          │                               │                          │
│          │ discovery                     │ announce                 │
│          ▼                               ▼                          │
│   ┌─────────────────────────────────────────────────┐               │
│   │              Indexers / DHT                     │               │
│   └─────────────────────────────────────────────────┘               │
│                                                                     │
│   ┌─────────────────────────────────────────────────┐               │
│   │              Caches (anyone)                    │               │
│   └─────────────────────────────────────────────────┘               │
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

t=-       Client discovers provider
          • Query indexer for providers accepting writes TODO: Really?
          • Or: use known/bookmarked provider
          • Or: query DHT for nearby providers
          Chain: read (verify provider's Proof-of-DOT, stake, bytes stored)

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
          • One transaction covers thousands of writes
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

The key insight: the chain exists as a credible threat, not as the happy path. Rational providers serve data directly because the alternative (on-chain dispute) is expensive for everyone and catastrophic for cheaters.

---

## Proof-of-DOT Foundation

Before discussing storage mechanics, we need identity and sybil resistance. Without it, reputation is meaningless, spam is free, and accountability is impossible.

Proof-of-DOT (detailed in the [Proof-of-DOT Infrastructure Strategy](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/)) provides this foundation:

**For clients:**
- Lock DOT against a PeerID
- Providers can quickly lookup PeerIDs for proof of DOT on connection establishment
- Enables reputation: providers remember past interactions with this identity
- Prevents spam at scale: creating identities gets costly for spammers

**For providers:**
- Same Proof-of-DOT mechanism as clients (sybil resistance, identity)
- *Separately*: providers lock collateral for availability contracts (see [Availability Contracts](#availability-contracts))
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
3. Both parties sign a commitment: `{contract_id: Option, mmr_root, start_seq}`
4. Done - data is stored and commitment is provable

The `contract_id` is optional:
- **With contract:** References an on-chain contract with stake, duration, and slashing terms. Full economic guarantee.
- **Without contract (`None`):** Best-effort storage. Provider serves based on reputation and payment priority. No slashing, but the signed commitment still proves the provider accepted the data.

### Provider Terms

Providers advertise their terms via metadata endpoint:

```
{
  free_storage: { max_bytes: 1GB, max_duration: 24h, per_peer: true },
  paid_storage: { price_per_gb_month: 0.01 DOT },
  contract_required: false,  // or true for contract-only providers
  min_stake_ratio: 0.001,    // DOT per GB for contracts
}
```

Clients choose based on needs:
- Ephemeral chat image → use free tier, no contract
- Important backup → pay for contract with high-stake provider

### Why Contracts Matter

Without contract: Provider can drop data anytime. Your only recourse is reputation - stop using them, tell others.

With contract: Provider has stake at risk. You can challenge anytime. Cheating = slashing. The signed commitment + on-chain contract = economic guarantee.

```
Guarantee Spectrum:
═══════════════════════════════════════════════════════════════════

No contract, no payment:
  Provider might keep it, might not. Free tier, best effort.

No contract, with payment:
  Provider prioritizes you. Still no slashing, but reputation matters.

Contract with low stake:
  Some economic guarantee. Cheap, but slashing hurts less.

Contract with high stake:
  Strong guarantee. Provider loses significant value if caught cheating.
```

The contract (or lack thereof) determines what recourse you have if the provider misbehaves.

---

## Availability Contracts

### Contract Lifecycle

```
1. NEGOTIATION (off-chain)
   ────────────────────────────────────────────────────────
   Client selects provider based on:
   • Price per byte per duration
   • Stake-per-byte ratio (higher = more collateral at risk)
   • Reputation (past performance, challenge history)
   
   Client uploads data, provider acknowledges receipt

2. COMMITMENT (on-chain, one transaction)
   ────────────────────────────────────────────────────────
   Provider commits MMR root covering client's data
   • Batched: one commit can cover many clients' data
   • Stake locked proportional to committed bytes
   • Contract terms recorded: duration, client, data size

3. ACTIVE PERIOD (off-chain)
   ────────────────────────────────────────────────────────
   Provider stores data, serves reads
   Client can:
   • Read data anytime (off-chain, priority based on payment)
   • Verify randomly (off-chain, request chunks and check proofs)
   • Challenge formally (on-chain, if off-chain verification fails)

4. GRACE PERIOD (contract approaching end)
   ────────────────────────────────────────────────────────
   Duration: 7-30 days before contract end
   Provider must continue serving
   Client can:
   • Extend contract (if terms allow)
   • Migrate data to new provider
   • Let it expire

5. SETTLEMENT (on-chain, one transaction)
   ────────────────────────────────────────────────────────
   Provider requests payment release
   Client has final challenge window, then:
   • Pay in full → provider receives payment
   • Partial pay → remainder burned (not paid to provider)
   • Don't pay → all burned
   • Challenge → dispute resolution
```

### The Burn Option: Lose - Lose

At contract end, the client has locked funds that would normally go to the provider. The client can choose to:

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

Burning is for genuine grievances: the provider technically met their availability commitment (not slashable) but provided poor read service, slow responses, or other quality issues. It's a reputation signal that costs the client nothing extra (beyond losing the provider relationship) but denies the provider revenue they arguably didn't earn.

### On-Chain Challenge

If the provider did not only do a bad service in providing the data, but actually lost it, the client has stronger options than just not paying:  It can force provider to prove data availability on-chain:

```
1. Client initiates challenge
   • Specifies chunks to prove
   • Deposits 75% of estimated challenge cost

2. Provider responds (must respond within deadline)
   • Submits Merkle proofs for challenged chunks
   • Pays 25% of challenge cost from staked collateral (enforced by protocol)

3. Outcomes:
   
   A. Provider proves successfully:
      • Client "loses" their 75% deposit
      • Provider "loses" their 25%
      • But: client now has the data (recovered via proof)
      • Net: expensive for both, but data recovered
   
   B. Provider fails to prove:
      • Provider's contract stake fully slashed
      • Client refunded their deposit + compensation from slash
      • Clear on-chain evidence of fraud
```

**Dynamic cost split based on response time:**

Base ratio is 75% challenger / 25% provider, but shifts based on how quickly the provider responds:
- Fast response → split shifts toward challenger (e.g., 85/15), rewarding responsiveness
- Slow response → split shifts toward provider (e.g., 50/50), penalizing delay
- Timeout (1-2 days) → full slash

This incentivizes quick responses without requiring impossibly short deadlines. For actual data recovery, clients send parallel requests anyway—not waiting per-chunk.

Why this works:
- Provider always pays *something* (deterrent for ignoring off-chain requests)
- Attacker pays more than victim in base case (griefing is expensive)
- Provider's total exposure is bounded by their stake (they chose this risk level)
- Large-scale attacks are visible on-chain; governance can intervene in such an unlikely scenario

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
| Commit deadline | TBD | Depends on exact commit/activation flow (see Open Questions) | TODO
| Challenge response | Up to ~1-2 days | Cost split shifts toward provider the longer they take |
| Grace period | 7-30 days | Time for client to migrate data |
| Settlement window | 3 days | Time for final challenges |
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

### Comparison: Filecoin Retrieval vs. Our Model

Filecoin's [retrieval market](https://spec.filecoin.io/systems/filecoin_markets/retrieval_market/) uses [payment channels](https://spec.filecoin.io/systems/filecoin_token/payment_channels/) with incremental payments:
- Client creates payment channel on-chain, locks funds for entire retrieval upfront
- Provider sends data in chunks, pauses when payment needed
- Client sends signed vouchers (cumulative value, increasing nonce) as data arrives
- Provider submits final voucher on-chain within 12hr window to collect
- Vouchers are channel-specific (client → provider), so no double-spend risk

Our model is simpler:
- Pay based on relationship/upfront
- No per-request cryptographic receipts  
- If unhappy: switch providers, stop paying

**What each approach actually provides:**

Both models solve the same problem: batching micropayments to avoid per-request on-chain transactions. They differ in how they manage risk.

| Aspect | Payment Channels | Our Model |
|--------|------------------|-----------|
| Trust model | Cryptographic (signed vouchers) | Reputation (track record) |
| Stranger interactions | Built for this | Small initial exposure, grow with trust |
| State to track | Channel state, vouchers, nonces | Simple: received/served ratio |
| Settlement | Explicit on-chain close | None needed (prepaid) |
| Risk granularity | Per-voucher (arbitrarily fine) | Per-prepayment (relationship-based) |

Payment channels let you control risk at arbitrary granularity within a single transfer—you release payment incrementally as data arrives. If the provider stops, you've only authorized payment for what you received so far. Note: this is pure risk control, not dispute resolution. Vouchers prove what you *signed*, not what you *received*. Neither system can prove delivery.

Our model controls risk differently: start with small prepayments to unknown providers, increase as trust grows. The maximum exposure is capped by how much you've prepaid, which is proportional to demonstrated track record.

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

**Overhead:**

Payment channels add per-interaction overhead: voucher signing and validation on every payment interval. For cheap services where even aggressive intervals (e.g., every 1MB) yield sub-cent payments, this overhead exceeds the value being transferred.

For longer relationships, you could use larger intervals (e.g., daily vouchers), but then you're not getting fine-grained risk control anyway—and you still need the channel infrastructure, settlement deadlines, and locked funds.

Our model has minimal overhead: one on-chain transfer whenever the client wants to top up. No vouchers, no deadlines, no locked funds.

**User experience:**

Payment channels require users to:
- Decide upfront how much to lock per provider
- Manage channel lifecycles (open, close, reopen for different providers)
- Ensure funds are in the right channel before requesting service
- Monitor settlement windows (or risk losing unclaimed funds)

Our model: transfer some DOT to a provider when you want priority. Done. Switch providers freely—no channel management, no locked funds.

**Why simplicity is sufficient for cheap services:**

Serving data over a network is cheap. Bandwidth costs for providers using affordable infrastructure (Hetzner, OVH, etc.) are roughly €0.001/GB—a tenth of a cent. Even with markup for profit, storage, and operations, gigabyte-scale transfers cost cents, not euros.

Our risk profile:

- New provider: prepay ~10-30 cents, get substantial read access (tens of GB)
- Established provider: prepay a euro or two, get priority access
- Maximum exposure: a few euros to your most trusted providers

This is acceptable because there's symmetry: providers invest in building reputation by offering free/cheap service initially. A provider who "exits" with remaining credits is taking money, but they also provided substantial service to earn that trust. It's arguably a legitimate business model—build reputation, provide service, eventually retire and keep the final credits. (And credits only mean priority—they still served *something*.)

**What we accept:**

A provider who systematically underperforms might go undetected longer than with fine-grained voucher tracking. But cumulative reputation effects (clients leaving, lower priority scores) still punish this behavior. We trade granular accounting for simplicity, betting that small exposure limits and easy exit are sufficient for cheap services.

**Why not use payment channels for storage/availability contracts?**

One might consider payment channels for long-term storage: client releases payments incrementally over the contract lifetime, stops paying if problems arise. But this creates new problems:

- Client must come online regularly to release payments
- If client disappears, provider faces a dilemma: delete data (breaking availability) or continue serving unpaid (hoping for late payment)
- Long lockup periods tie up client funds
- The "store and forget" use case breaks entirely

Our upfront payment with challenge mechanism fits better: client pays upfront, provider commits via stake, availability is enforced via challenges. The client doesn't need to be online to "release" payments—they just need to be able to challenge if something goes wrong.

**Future evolution:**

Payment channels remain an option for services where:
- Per-interaction amounts are significant (not sub-cent)
- Fine-grained risk control within a single interaction matters
- The overhead is justified by the amounts at stake

For cheap reads and long-term storage, neither condition holds. No reusable payment channel library exists for Substrate anyway—Filecoin's [go-fil-markets](https://github.com/filecoin-project/go-fil-markets) is Go and tightly coupled to Filecoin, [Rust-Lightning (LDK)](https://github.com/lightningdevkit/rust-lightning) is Bitcoin-specific. Building from scratch isn't justified when the simple model covers our use cases.

### Challenge Economics

The challenge mechanism creates a price ceiling that protects clients even when dealing with a single provider (monopoly on specific data).

**The client's calculus:**

A client who needs data has two options:
1. Pay the provider's asking price
2. Challenge on-chain and recover the data via proof

If the provider demands more than the challenge cost, the rational client challenges. The client pays roughly the same (challenge cost ≈ ransom price), but the provider:
- Pays 25% of challenge cost (direct loss)
- Gets no payment for serving
- Suffers reputation damage (failed to serve, forced on-chain)

**The provider's calculus:**

A rational provider prices *below* the challenge threshold because:
- Price below threshold → earn payment, client happy, reputation intact
- Price at/above threshold → client challenges, provider pays 25%, earns nothing, reputation damaged

The ceiling is approximately `challenge_cost` per read. Above this, clients prefer the "nuclear option."

**In practice:**

Most of the time, competition drives prices well below this ceiling. The challenge economics only matter for the edge case: a single provider holding unique data and attempting to extract monopoly rents. The ceiling prevents ransom attacks without requiring continuous oversight.

### Caching

Anyone can cache and serve data. The protocol doesn't distinguish between "origin" and "cache"—it's all just nodes serving bytes.

**Why caching works:**

- Caches fetch data like any client
- Once cached, they compete with the origin for read payments
- Origin can't block caches (can't distinguish them from regular clients)
- Caches earn by serving—no revenue split needed, no permission required

**Privacy:** The protocol sees opaque bytes. Private data is encrypted at the application layer. A cache doesn't know or care whether it's serving a public image or an encrypted backup chunk. This keeps the protocol simple and uniform.

---

## Discovery

How does a client find who can serve content?

### Chain-Based Discovery

The chain stores the mapping from data roots to providers:

```
On-chain storage:
{
  data_root → [provider_1, provider_2, ...]
}
```

**Setting a provider:** Clients submit a transaction with the data_root and provider list. The chain verifies that each provider has signed the commitment before accepting. This prevents clients from claiming providers store data they don't.

**Lookup:** Query the chain (or a chain indexer) by data_root → get provider endpoints. Connect directly.

**Why this is sufficient:**

1. **Chat/messaging:** Roots are shared in messages along with provider info. No chain lookup needed.
2. **Websites:** Domain → data_root mapping on-chain. One lookup per site.
3. **Backups:** Client knows their own providers. No discovery needed.
4. **Shared workspaces:** Contract is on-chain, providers are in the contract.

Most use cases either share provider info directly (peer-to-peer) or do a single chain lookup (public content).

### Scaling Access

Want content served faster or more reliably? Add more providers:

- Store with multiple providers (they all sign the commitment)
- Clients pick whichever provider has best latency
- Trial and error, or providers expose latency hints via metadata

No separate "cache" layer needed. More providers = wider access = better availability.

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

Each client-provider relationship has an associated MMR (Merkle Mountain Range) of data versions, giving each client their own version history.

**Signed payload:**

```
{
  contract_id,    // Option: reference to on-chain contract, or None for best-effort
  mmr_root,       // root of MMR containing all data_roots
  start_seq,      // sequence number of first leaf in this MMR
}
```

Both client and provider sign this payload. The dual signatures implicitly commit to everything in the MMR.

- **With `contract_id`:** The commitment is enforceable via on-chain challenge. Provider stake is at risk.
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

The sequence number for any leaf is derived as `start_seq + mmr_position`, so it doesn't need to be stored explicitly. This saves space proportional to MMR size while adding only one field to the signed payload.

By signing the MMR root and start_seq, both parties commit to all leaves and their implicit sequence numbers. One signature covers the entire history.

**Commitment flow:**

1. Alice uploads new data → new `data_root`
2. Provider appends `data_root` as new leaf in MMR
3. Provider computes new `mmr_root`
4. Both sign `{contract_id, mmr_root, seq, data_size, total_size}`
5. Commitment complete

**On-chain storage is optional:**

The signed commitment is valid off-chain. Alice can:
- Keep it locally (minimal trust assumption)
- Checkpoint on-chain (convenience, discoverability)

The contract on-chain stores terms and optionally the latest checkpointed MMR root. Challenges work with or without on-chain checkpoints—signatures prove validity.

### Challenge Flow

1. Alice presents: signed `{contract_id, mmr_root, seq:5, ...}` + "challenge chunk X of data_root_v3"
2. Alice provides: MMR inclusion proof that `data_root_v3` is leaf 3 under `mmr_root`
3. Provider must: serve chunk X with Merkle proof to `data_root_v3`

**Deletion defense:**

If data was legitimately deleted (client started fresh MMR):

1. Provider shows: signed `{contract_id, mmr_root_new, seq:8, ...}` with higher seq
2. Provider proves: seq 3 is not in `mmr_root_new` (i.e., `3 < start_seq`)
3. Challenge rejected

The MMR structure itself proves whether old data still exists. No separate deletion tracking needed.

### Updates and Deletions

**Updates (extending):**

Most updates extend the existing MMR:
- New data_root appended as new leaf
- Old leaves remain valid and challengeable
- Unchanged chunks deduplicated in storage

**Deletions (fresh MMR):**

Client can start a fresh MMR to delete old data:
- New commitment with seq N+1 but MMR containing only new data
- Old data_roots no longer in MMR → provider can/should delete
- Challenge against old data_root fails (provider shows it's not in current MMR)

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

## Use Cases

### Media in Chat

**Scenario:** Group chat where members share photos and videos.

**Requirements:** Low latency, shared access, participants can contribute storage costs.

**Flow:**

```
1. Channel setup (once):
   • Creator sets up on-chain storage contract with a provider
   • Contract specifies: admin (creator), participants (append-only), providers
   • Creator pays initial deposit

2. Adding members:
   • Admin adds user public keys to contract on-chain
   • New members can be added as paying participants (chip in for storage)
   • Permissions: admin can delete, participants can only append

3. Alice sends a photo:
   • Chunks and encrypts photo with channel key
   • Appends to channel's MMR (append-only for participants)
   • Sends message: { mmr_root, seq, offset, length, filename }

4. Bob receives message:
   • Looks up contract on-chain → gets provider endpoint
   • Requests chunks from provider
   • Verifies against mmr_root
   • Decrypts with channel key
```

**Discovery:** Provider is in the contract. No separate lookup needed.

**Cost sharing:** Participants added as payers contribute to storage costs. The contract tracks who pays what.

**Latency:** Sub-second for write and read.

### Personal Backup

**Scenario:** User backs up ~/Documents (50GB).

**Requirements:** High availability, write-heavy, read-rare (only on recovery).

**Flow:**

```
1. Initial backup:
   • Recursively chunk all files
   • Build directory Merkle tree
   • Encrypt everything with user's master key
   • Select 2-3 diverse providers (check independence)
   • Upload to all providers in parallel
   • Request availability contracts from all
   • Providers commit MMR roots

2. Incremental backup (next day):
   • Scan for changed files
   • Re-chunk only modified files (content-defined chunking helps)
   • Most chunks unchanged → not re-uploaded
   • Upload new chunks, build new directory tree
   • New root committed to MMR

3. Verification (ongoing):
   • Periodically pick random chunks
   • Request from each provider
   • Verify against known roots
   • Alert if verification fails

4. Recovery (rare):
   • Fetch directory tree from any provider
   • Distribute chunk downloads across providers in parallel
   • Verify all chunks against known roots
   • Decrypt and reconstruct
```

**Tier:** Availability contracts with 2-3 providers.

### Public Website / CDN

**Scenario:** Developer deploys static site (500MB of assets).

**Requirements:** Read-heavy, global distribution, low latency.

**Flow:**

```
1. Publish:
   • Bundle site assets → chunks → Merkle tree → root
   • Upload to 1-2 storage providers
   • Transient storage or availability contract
   • Publish DNS: site.example.com → root hash

2. First visitor (cold cache, Tokyo):
   • DNS → root hash
   • Query indexer → find providers (maybe US-based)
   • Fetch chunks from provider
   • Tokyo-area caches store chunks

3. Subsequent visitors (warm cache, Tokyo):
   • Query indexer → find providers + Tokyo cache
   • Fetch from Tokyo cache (faster)
   • Popular assets cached globally

4. Update:
   • Build new site → new root
   • Update DNS: site.example.com → new root
   • Old caches still serve old version (immutable by hash)
   • New version gradually populates caches
```

**Tier:** Paid Writes or Availability. Heavy reliance on caching.

### Business Backup (Compliance, SLA)

**Scenario:** Enterprise backs up critical data with recovery time guarantees.

**Requirements:** High availability, SLA, audit trail, compliance.

**Flow:**

```
1. Setup:
   • Select 5 providers (diversity required)
   • Erasure coding: 3-of-5 scheme
   • Availability contracts with Premium SLA terms
   • Encryption with enterprise key management

2. Backup:
   • Chunk and encrypt data
   • Erasure encode: split into 5 shards
   • Each shard → different provider
   • Each provider commits their shard

3. Monitoring:
   • Continuous verification sampling
   • Track response times (SLA compliance)
   • Alert on failures or SLA violations
   • Automatic failover planning

4. Recovery:
   • Parallel fetch from all 5 providers
   • Need only 3 responding to reconstruct
   • SLA guarantees recovery time
   • Violations → contractual penalties

5. Compliance:
   • All Merkle roots on-chain (audit trail)
   • Challenges prove data existed at specific times
   • Provider slashing provides accountability
```

**Tier:** Availability with erasure coding across 5 providers.

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

The on-chain path is expensive—submitting Merkle proofs with actual chunk data costs significant gas and is slow/has limited bandwidth. This makes recovery of large data unrealistic, but it puts econimic pressure on providers, which makes the cheap direct-serving path incentive-compatible.

A rational provider always serves directly because:
- Direct serving: earn payment, bandwidth cost only
- Being challenged: pay 25%+ of challenge cost, lose reputation, data served anyway via chain

The expensive on-chain path is the "nuclear option" that prevents ransom attacks. It exists to never be used.

### Saturn and Storacha: Filecoin's Hot Storage Layer

[Saturn](https://saturn.tech/) and [Storacha](https://storacha.network/) represent Filecoin's evolution toward hot storage and fast retrieval—similar goals to this design.

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

**Comparison:**

| Aspect | Saturn/Storacha | This Design |
|--------|-----------------|-------------|
| Sybil resistance | Orchestrator approval | Proof-of-DOT |
| Payment model | Pooled monthly payouts by metrics | Direct client→provider, priority by ratio |
| Node membership | Centralized orchestrator | Permissionless with stake |
| Storage proofs | PDP (periodic) | Challenges (on-demand) |
| Delivery enforcement | Reputation + SPARK testing | On-chain challenges with slashing |
| Discovery | IPFS DHT + indexers | Proof-of-DOT indexers + DHT fallback |

**Key differences:**

Saturn's centralized orchestrator is a pragmatic choice for bootstrapping—it manages quality control and payment distribution. Our design is more decentralized from the start, using Proof-of-DOT for permissionless participation and on-chain mechanisms for enforcement.

Storacha separates storage/indexing/retrieval into distinct node roles. Our design allows any provider to serve all roles, with specialization emerging from economics rather than protocol requirements.

The most significant difference is enforcement: Saturn/Storacha rely on reputation and testing (SPARK) to ensure quality. Our design has on-chain challenges as a fallback—expensive, but a credible threat that makes off-chain cooperation incentive-compatible.

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

Filecoin launched [Proof of Data Possession (PDP)](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/) on mainnet in May 2025. This raises the question: will PDP replace PoRep?

**Answer: No. They're complementary, not competing.**

The fundamental difference is what each proves:

| Property | PoRep | PDP |
|----------|-------|-----|
| Unique copy | Yes (sealed, incompressible) | No (can be shared/compressed) |
| Immediate access | No (must unseal, ~3 hours) | Yes (raw data) |
| Consensus-safe | Yes | No |
| Mutable | No (sector is sealed) | Yes (add/delete/modify) |

**Why PoRep can't be replaced:**

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
│                                             │
│   Cold Storage (PoRep + PoSt)               │
│   • Archival, long-term                     │
│   • Sealed sectors, 32GB                    │
│   • Consensus participation                 │
│   • ~3 hour unsealing for retrieval         │
│                                             │
├─────────────────────────────────────────────┤
│                                             │
│   Hot Storage (PDP)                         │
│   • Fast retrieval                          │
│   • No sealing, raw data                    │
│   • Mutable collections                     │
│   • No consensus weight                     │
│                                             │
└─────────────────────────────────────────────┘
```

**What this means for us:**

We're designing for the *hot storage* use case—fast reads, mutable data, no sealing. Filecoin's PDP validates that there's demand for this. But we differ in two ways:

1. **Challenge frequency:** PDP still requires ongoing proofs (lightweight, but continuous). We use rare, on-demand challenges.

2. **Data delivery enforcement:** PDP proves possession but doesn't force serving. Our challenges require submitting actual data on-chain.

The continuous vs. on-demand distinction matters for scalability. PDP's ~415KB proofs are cheap per-proof, but multiply by every provider every period and chain load adds up. Our approach: zero chain load in the happy path.

### What We Give Up: The Game-Theoretic Trade-off

Our design bets on rational adversaries. This is a weaker security model than continuous cryptographic proofs. What could go wrong?

**Attack: The Lazy Provider**

A provider accepts data, commits the Merkle root, then deletes most of it—betting that:
1. Most clients never challenge (true in practice)
2. The few who challenge can be paid off or ignored
3. Expected profit from cheating > expected loss from slashing

This attack is *rational* if `P(challenge) × slash_amount < storage_cost_saved`.

**Our mitigations:**

1. **Challenge cost asymmetry.** Challenger pays 75%, provider pays 25%. But if provider fails, they lose *entire stake*. Expected loss for cheating = `P(challenge) × full_stake`, which dominates storage savings for any reasonable challenge probability - for sensible sized stake, but rational clients will prefer higher staked providers.

2. **Reputation visibility.** Failed challenges are on-chain. A provider with even one failed challenge is radioactive—no rational client stores with them. The reputational loss exceeds any single-contract gain.

**Attack: The Malicious Challenger**

A competitor or griefer challenges a honest provider repeatedly to drain their funds or force them offline.

**Our mitigations:**

1. **Challenger pays more.** 75/25 split means griefing is expensive.
2. **Provider wins if honest.** Successful proof means challenger "loses" their deposit (data recovered, but at high cost).
3. **Threshold for repeated challenges.** Provider exposure is limited by stake. Once stake is gone, provider can't be forced to pay the 25% anymore. We could limit this to only a fraction of the stake too, so large stake is less risky and serves the "do not lose data" property better.

**Attack: Data Withholding After Commitment**

Provider commits root, client pays, provider refuses to serve—betting client won't pay challenge cost.

**Our mitigations:**

1. **Challenge recovers data.** Unlike Filecoin, our challenge *forces* data on-chain. Provider can't just prove possession—they must submit the actual chunks.
2. **Challenge cost < ransom cost.** If provider demands ransom > challenge cost, rational client challenges.
3. **Provider pays too.** Even "winning" a challenge costs the provider 25%. No profit in forcing challenges.

**What we can't prevent:**

- **Irrational adversaries.** Someone willing to lose money to hurt others. Mitigated by stake requirements (expensive to be irrational at scale).
- **Lazy clients.** If no one ever challenges, the deterrent weakens. Mitigated by making challenges profitable when they catch cheaters.

**The honest assessment:**

Continuous proofs (Filecoin PoSt, StorageHub) catch *every* cheater, every time. We catch cheaters *probabilistically*, when challenged. This is weaker. We accept this trade-off because:

1. Chain load scales with storage volume in continuous systems. Ours scales with disputes (rare).
2. Hardware requirements are lower (no GPU, no sealing).
3. The game theory is sound: rational providers don't cheat when cheating has negative expected value.

For archival storage where "never lose a bit" matters more than cost, use Filecoin with PoRep. For hot storage where performance and cost matter, the game-theoretic model is sufficient.

### Encrypted Data: PoRep Guarantees Without Sealing

For private/encrypted data, we can achieve Filecoin's uniqueness and incompressibility guarantees *without* slow sealing.

**The scheme (needs cryptographic review):**

```
mask = PRF(provider_key, client_private_key)
stored_data = encrypted_data XOR mask
```

Each provider receives different bytes to store, because each has a different `provider_key` and the mask is derived using the client's private key. The exact PRF construction needs review to ensure that observing many `stored_data` values doesn't leak information about the mask.

**What the provider has:**
- `stored_data` (what they must keep)
- `provider_key` (their own key)

**What the provider does NOT have:**
- `client_private_key`
- The XOR mask
- Any way to reconstruct `stored_data`

**This gives us:**

| Property | Filecoin PoRep | Our Encrypted Scheme |
|----------|----------------|---------------------|
| Unique per provider | Yes | Yes |
| Incompressible | Yes | Yes |
| No cross-provider sharing | Yes | Yes |
| Sealing time | ~1.5 hours | Seconds |
| Unsealing time | ~3 hours | Seconds |
| GPU required | Yes | No |

**Why Filecoin needs slow sealing but we don't:**

Filecoin's problem: the provider knows the sealing algorithm. Given enough time, they could delete data and regenerate it when challenged. Slow sealing makes "enough time" longer than the challenge window.

Our scheme: the provider *never* knows the mask (it's encrypted with the client's private key). No amount of compute helps them. They either stored the exact bytes or they didn't.

The asymmetry comes from cryptographic secrets, not computational slowness.

**Combined with slashing:**

If a provider loses even one byte (disk failure, deletion, corruption), they cannot reconstruct it. A challenge for that byte means full slash. This creates strong incentive for providers to maintain redundancy (RAID, backups) on their end.

**For public data:**

This scheme doesn't work—anyone can compute the mask. But: **PoRep for public data doesn't solve the real problem either.**

Consider: who pays for redundant copies of public data?

| Data State | What Happens | Redundancy |
|------------|--------------|------------|
| Popular/valuable | Many nodes cache it (serving is profitable) | Natural, incentive-driven |
| Unpopular | No profit in serving | Who pays to store it? |

For unpopular public data, we have a tragedy of the commons. The data belongs to no one, so no one internalizes the benefit of preserving it. Filecoin's PoRep can *prove* that N unique copies exist, but it doesn't solve *who funds* those copies. Someone still has to pay for each storage deal.

Options for preserving public data (same in both systems):
1. **Altruism** — someone pays because they care
2. **Collective funding** — DAO/foundation pays
3. **The uploader pays** — but then it's not really "public" in the commons sense

**Private backup as insurance:**

If someone truly cares about preserving public data, they can store a private encrypted backup with availability contracts. This isn't public storage - it's personal insurance. If public availability ever fails (caches expire, no one serves it anymore), they can republish from their backup. The data stays private until needed, so our encrypted-data guarantees apply fully.

**Our position:**

Public cold storage with proven redundancy is not our primary objective. We optimize for hot storage: fast reads, low latency, mutable data. For that use case, natural caching of popular content works well.

For public data that needs long-term archival with provable redundancy:
- **Encrypt it** — become the "owner," use our system with full guarantees
- **Use Filecoin** — their PoRep is designed exactly for this use case

We don't need to be better everywhere. Cold storage of public data can live on Filecoin; it doesn't matter where archival data sits as long as it's preserved. Our value is in the hot path: writes without consensus, fast reads, game-theoretic enforcement without continuous proofs.

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

**Phase 1: Free Service**

We provide storage/read service for free via existing Parity/ecosystem infrastructure. No Proof-of-DOT, no payments. Just a working service that applications can build on. This establishes:
- Working protocol and implementation
- Initial user base
- Baseline performance expectations

**Phase 2: Introduce Proof-of-DOT**

Add Proof-of-DOT for sybil resistance. Users who register get priority over anonymous users. Still free, but differentiated service quality. This establishes:
- Identity layer
- Quality-of-service differentiation
- User familiarity with Proof-of-DOT mechanism

**Phase 3: Introduce Payments**

Add payment tracking. Providers prioritize paying clients over free-tier. This effectively opens the system to third-party providers—before this phase, other providers *could* join but had no economic incentive. Now there's revenue to compete for. This establishes:
- Economic sustainability
- Provider competition
- Market-driven pricing

**Phase 4: Availability Contracts**

Add on-chain availability commitments with challenge mechanism. For users who need guarantees beyond reputation. This establishes:
- Full trust model
- Enterprise-grade guarantees
- Complete feature set

**Why this works:**

Each phase is functional on its own. There's no circular dependency (need users to attract providers, need providers for users). The system works at every stage—it just gets better as it evolves.

Providers who join early build reputation before competition intensifies. Users who join early get free/cheap service while the market develops. Everyone has incentive to participate at every phase.

---

## Summary

The design achieves scalability by removing the chain from the hot path:

| Operation | Chain Interaction | Frequency |
|-----------|-------------------|-----------|
| Write | None | High |
| Read | None | High |
| Contract setup | One tx | Low |
| Challenge | Only if fraud | Rare |

**Key design choices:**

1. **Unified write model.** Every write follows the same path: upload, merkleize, sign commitment. The `contract_id` in the commitment is optional - with contract you get slashing guarantees, without you get best-effort storage. One protocol, continuous spectrum of guarantees.

2. **Protocol-layer opacity.** Providers see only content-addressed chunks. File structure, metadata, directories—all application-layer concerns. Privacy by design: encrypt everything, provider learns nothing.

3. **MMR-based commitments.** Per-client Merkle Mountain Ranges track version history. Both parties sign `{contract_id: Option, mmr_root, start_seq}`. One signature covers entire history. Deletions via fresh MMR with higher start_seq.

4. **Proof-of-DOT foundation.** Sybil resistance via DOT staking enables identity and reputation. Payment history (not stake) determines service priority. Simple prepayments, no payment channels needed.

5. **Game-theoretic enforcement.** Challenges replace continuous proofs. Rational providers serve data because being challenged is expensive. The burn option lets clients punish bad service at their own cost—no proof needed, just reputation loss.

6. **Chain-based discovery.** Providers are registered on-chain per data_root. For peer-to-peer sharing, provider info travels with the content reference. No separate indexing infrastructure needed.

The chain exists as a credible threat. Rational actors never use it.
