# Scalable Web3 Storage

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Draft |
| **Version** | 2.0 |
| **Related** | [Implementation Details](./scalable-web3-storage-implementation.md), [Proof-of-DOT Infrastructure Strategy](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/) |

---

## Executive Summary

**The core insight:** Storage isn't free—someone pays, so someone naturally cares. That paying stakeholder verifies their own data as a byproduct of use. Stronger guarantees and decentralization emerge naturally from data importance: when data matters enough, more users/businesses add replicas, each with significant stake at risk. A bucket replicated by major providers across multiple jurisdictions is practically guaranteed by the economic stakes each provider would lose. And you never need to trust: you can always verify yourself or add your own replica.

**What we're building:** A bucket-based storage system where providers lock stake and clients can challenge. The chain exists as a credible threat, not as the hot path. Normal operations (reads, writes, storage) happen directly between clients and providers. The chain is only touched for setup, checkpoints, and disputes.

**Why it's different:** Existing Web3 storage either proves too much (Filecoin's continuous cryptographic proofs—heavy, slow) or guarantees too little (IPFS—no persistence). We use game theory: rational providers serve data because being challenged costs more than storage savings. This scales with provider capacity, not chain throughput.

---

## Table of Contents

1. [The Problem with Web3 Storage](#the-problem-with-web3-storage)
2. [Our Approach: Pragmatic Verification](#our-approach-pragmatic-verification)
3. [Architecture Overview](#architecture-overview)
4. [Economic Model](#economic-model)
5. [Proof-of-DOT and Read Incentives](#proof-of-dot-and-read-incentives)
6. [Client Strategies](#client-strategies)
7. [Data Model](#data-model)
8. [Use Cases](#use-cases)
9. [Comparison with Existing Solutions](#comparison-with-existing-solutions)
10. [Rollout](#rollout)
11. [Future Directions](#future-directions)

---

## The Problem with Web3 Storage

Decentralized storage faces a fundamental tension: **guarantees vs. throughput**.

### The Heavyweight Approach: Filecoin

Filecoin provides strong cryptographic guarantees. Providers "seal" data into 32GB sectors—a process taking ~1.5 hours with specialized GPU hardware. Every 24 hours, every sector must be proven on-chain via Proof-of-Spacetime.

This works for cold archival storage. But for interactive applications:
- **Write latency**: Minutes to hours (sealing)
- **Hardware**: GPU required, commodity hardware insufficient
- **Chain load**: Every sector, every day, proven on-chain
- **Rigidity**: 32GB sectors don't fit small files; padding wastes space

Filecoin's newer PDP (Proof of Data Possession, May 2025) improves hot storage, but still requires periodic proofs. The fundamental constraint remains: **chain throughput bounds storage capacity**.

### The Lightweight Approach: IPFS

IPFS provides content addressing and a peer-to-peer network. Data is identified by hash, seemingly removing provider dependence.

But content hashes are just names. Nothing requires anyone to store the data:
- **No persistence**: Nodes can drop data immediately
- **No read incentives**: Why serve data to strangers for free?
- **Slow discovery**: DHT lookups take 2-10 seconds, often fail
- **No accountability**: A hash with no providers is a dangling pointer

### The Common Thread

Both approaches struggle with the same question: **How do we know data is available?**

Filecoin answers with cryptography: continuous proofs. This is expensive and doesn't scale.

IPFS doesn't answer at all: hope someone cares enough to store it.

Neither provides fast, reliable discovery. Neither handles the read incentive problem well. And for chain-bnased proofs, throughput fundamentally limits capacity.

---

## Our Approach: Pragmatic Verification

We start from a different premise.

### Storage Only Matters If Someone Cares

Consider: what does it mean to "guarantee" storage of data no one ever reads?

If a file sits untouched for years—never read, never verified, never needed—what exactly is being guaranteed? The cryptographic proof that some provider has *something* on disk? But the practical guarantee—that useful data remains accessible—requires someone to actually want it.

**Our design assumption:** A rational client who paid for storage will periodically verify their data exists. By reading it. By spot-checking random chunks. If no one ever checks, the data has no practical value worth guaranteeing.

This isn't a limitation. In reality, most data written is never read again—but that's fine. Someone still cares enough to pay for storage. Verification is automated and free: when you update your backup, the software spot-checks random existing chunks in the background. You might never restore that backup (hopefully you won't need to), but each incremental update verifies storage at no cost to you. The key is that this isn't fire-and-forget—there's ongoing interaction, even if minimal, and that interaction enables verification.

### Self-Interested Clients as Verification Layer

Traditional approach: An indifferent chain continuously verifies all storage, regardless of whether anyone cares.

Our approach: Interested clients verify their own storage, as a byproduct of using it.

The math is compelling. Suppose a client spot-checks 3 random chunks weekly. If a provider deletes 10% of their data:
- Probability of missing deletion per week: 0.9³ = 72.9%
- After 3 months (13 weeks): 98% detection probability
- After 6 months (26 weeks): 99.97% detection probability

And that's just explicit spot-checking. Every normal read is also implicit verification. A backup app that restores files is verifying storage. A website visitor loading an image is verifying storage. The "verifier's dilemma" (verification is too expensive) disappears when verification is free bandwidth.

### Automation Removes the Human Factor

"But users are lazy! They won't verify!"

Correct. Humans are unreliable. Software isn't.

The client software—the backup app, the file browser, the media player—performs verification automatically. When you open your backup app, it spot-checks a few random chunks in the background. When you browse a folder, the client fetches the directory listing chunks—verifying they exist and match their hashes. When you play a video, every chunk delivered is verified against its hash.

This happens without user action, without user awareness, without user discipline. The lazy human doesn't need to remember to verify. The software does it continuously, invisibly, as part of normal operation.

### Objective Reliability Emerges from Subjective Checks

All this subjective verification aggregates into objective reliability. There are two trust questions:

**Trusting a provider (for your own bucket)**: Providers have on-chain track records—agreements completed, extensions, burns, challenges received and failed. A provider with 100 successful agreements, 80% extension rate, and zero failed challenges is probably reliable—not because they claim to be, but because 100 paying clients verified them over time. (See [Client Strategies](#client-strategies) for practical selection criteria.)

**Trusting a bucket (someone else's data)**: How do you trust that a bucket you don't control will remain available? Look at who else cares:
- **Replica diversity**: How many independent providers are storing replicas? What are their stakes?
- **Stakeholder diversity**: Who funded these replicas? Major institutions? Community members? 
- **Data importance**: If this bucket has replicas from providers across multiple jurisdictions with significant stake, many parties have skin in the game.

The more independent stakeholders with economic interest in a bucket's availability, the stronger the guarantee—even if you never verify yourself.

### The Last Resort: Challenge It Yourself

What if you don't trust aggregate metrics? What if you have strict requirements?

**Add your own replica.** Anyone can create a replica agreement with any provider they choose. Pick a provider you trust, pay them directly, verify them yourself. Now you have at least one replica whose reliability you've personally established.

Or simply **challenge directly.** Anyone can challenge any provider for any data they have a commitment for. Don't trust that a provider still has your data? Challenge one random chunk. If they respond, you've verified (and recovered that chunk). If they don't, they get slashed, and the world learns they're unreliable.

The point: you're never dependent on trusting others' verification. You can always verify yourself, at any time, for any data you care about.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           ON-CHAIN                                  │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                         BUCKET                                │  │
│  │  ├── members: [Admin, Writers, Readers]                       │  │
│  │  ├── min_providers: 2                                         │  │
│  │  ├── snapshot: { mmr_root, start_seq, leaf_count }            │  │
│  │  ├── primary_providers: [A, B]  (admin-controlled)            │  │
│  │  └── storage agreements:                                      │  │
│  │      ├── Provider A: { Primary, max_bytes, payment, ... }     │  │
│  │      ├── Provider B: { Primary, max_bytes, payment, ... }     │  │
│  │      ├── Provider C: { Replica, sync_balance, last_sync, ... }│  │
│  │      └── Provider D: { Replica, sync_balance, last_sync, ... }│  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│    Chain touched for:                                               │
│    • Bucket creation and membership (once)                          │
│    • Storage agreement setup (per provider)                         │
│    • Checkpoints (infrequent, batched)                              │
│    • Replica sync confirmations (periodic)                          │
│    • Dispute resolution (rare, game-theoretic deterrent)            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ rare
                                    │
┌─────────────────────────────────────────────────────────────────────┐
│                          OFF-CHAIN                                  │
│                                                                     │
│   ┌─────────────┐    writes     ┌─────────────┐                     │
│   │   Client    │ ────────────> │  Primary    │                     │
│   │             │               │  Provider   │                     │
│   └─────────────┘               └─────────────┘                     │
│          │                             │                            │
│          │ reads                       │ sync                       │
│          ▼                             ▼                            │
│   ┌─────────────┐               ┌─────────────┐                     │
│   │  Primary or │               │   Replica   │ (syncs from         │
│   │  Replica    │               │   Provider  │  primaries/replicas)│
│   └─────────────┘               └─────────────┘                     │
│          ▲                                                          │
│          │ discovery: bucket → agreements → provider endpoints      │
│          └──────────────────────────────────────────────────────────│
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ foundation
┌─────────────────────────────────────────────────────────────────────┐
│                      PROOF-OF-DOT                                   │
│                                                                     │
│   Sybil resistance, identity, read priority                         │
│   See: Proof-of-DOT Infrastructure Strategy                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Buckets: Stable Identity in a Fluid Provider Market

The fundamental unit is the **bucket**—an on-chain container that groups related data.

```
Bucket (on-chain, stable identifier)
├── bucket_id: u64              // permanent, never reused
├── members: [Admin, Writers, Readers]
├── min_providers: u32          // quorum for checkpoints
├── primary_providers: [A, B]   // admin-controlled, max ~5
├── snapshot: { mmr_root, start_seq, leaf_count }
└── storage agreements → providers
```

**Why buckets, not just content hashes?**

A content hash names data but doesn't guarantee anyone stores it. A bucket makes availability explicit and controllable:

- **Stable identity**: The bucket_id never changes, even as providers come and go. Applications reference buckets, not providers. Switch providers without breaking links.

- **Explicit availability**: On-chain state shows exactly which providers have agreements. No guessing, no DHT lookups, no hoping.

- **Mutability with history**: Bucket contents evolve (new files, updates), but MMR commitments provide immutable snapshots at any point. Version N is always accessible even after version N+1 exists.

- **Permissionless persistence**: A frozen bucket (append-only) can be funded by anyone—not just the owner. You care about open-source documentation? Fund a replica. You care about historical records? Extend the agreements. Data survives even if the original owner disappears.

### Two Classes of Providers

Providers fall into two categories with different trust models:

**Primary Providers** (admin-controlled):
- Added only by bucket admin
- Receive writes directly from clients
- Count toward `min_providers` for checkpoints
- Limited to ~5 per bucket (prevents bloat)
- Can be early-terminated by admin (for misbehavior)

**Replica Providers** (permissionless):
- Added by anyone (you, a third party, a charity)
- Sync data autonomously from primaries or other replicas
- Paid per successful sync confirmation
- Unlimited count
- Cannot be early-terminated (run to expiry)

**Why this split?**

Writes need coordination—someone must order appends to the MMR. That's the admin's job, via primary providers they control.

But reads don't need coordination. Any provider with the data can serve it. Replicas provide permissionless read redundancy. Even if an admin is compromised or malicious, replicas ensure the data remains accessible from independent sources.

This creates a spectrum:
- **Centralized**: Single admin, few primaries, no replicas
- **Federated**: Admin with primaries, community-funded replicas
- **Permissionless**: Frozen bucket, anyone can add replicas, admin has no special power

### The Chain as Credible Threat

In normal operation, clients and providers interact directly:
- **Writes**: Client uploads chunks to provider, provider signs commitment
- **Reads**: Client fetches chunks from provider, verifies hashes
- **Storage**: Provider keeps data on disk, serves requests

The chain is touched only for:
- **Bucket creation**: Once
- **Agreement setup**: Per provider
- **Checkpoints**: Infrequent, batched (primaries)
- **Sync confirmations**: Replicas confirm they've synced to a checkpoint
- **Disputes**: Rare, expensive, avoided by rational actors

This inversion is key to scalability. Traditional approaches put everything on-chain (or prove everything to the chain). We put almost nothing on-chain—but the threat of on-chain resolution keeps everyone honest.

**Why does this work?**

On-chain challenges are expensive for everyone. The challenger must deposit funds. The provider must submit actual chunk data with Merkle proofs. Both sides pay transaction fees.

A rational provider prefers to just serve the data directly. Serving costs only bandwidth. Being challenged costs bandwidth *plus* on-chain fees *plus* time *plus* reputation damage. Even honest providers avoid challenges by being responsive.

The expensive on-chain path exists to make the cheap off-chain path incentive-compatible.

---

## Economic Model

### Storage Agreements

Every provider-bucket relationship is governed by a storage agreement:

```
StorageAgreement
├── owner: AccountId        // can top up, transfer ownership
├── max_bytes: u64          // quota for this provider
├── payment_locked: Balance // prepaid storage payment
├── price_per_byte: Balance // locked at creation
├── expires_at: Block       // when agreement ends
├── role: Primary | Replica
└── (replica only) sync_balance, sync_price, last_sync
```

**Binding commitment**: Neither party can exit early. Provider committed to store for the agreed duration. Client committed to pay for the agreed duration.

**Why binding?**
- Providers need predictability to provision storage
- Clients need assurance data won't be dropped mid-term
- Price volatility is handled by locking price at creation/extension

### Provider Stake

Providers register with a global stake that covers all their agreements:

```
Provider
├── stake: Balance          // total locked stake
├── committed_bytes: u64    // sum of max_bytes across agreements
├── stats: { agreements, extensions, burns, challenges_received, challenges_failed }
```

The stake-to-bytes ratio determines how much a provider can commit to. Higher stake = more capacity = more earning potential.

**Full stake at risk**: A single failed challenge slashes the provider's *entire stake*, not just the stake for that bucket. This makes cheating economics absurd—deleting 1% of data to save $0.12/year risks losing thousands of dollars in stake.

### The Challenge Game

When a provider doesn't serve data, clients can challenge on-chain:

```
1. Challenger initiates
   - Specifies: bucket, provider, leaf_index, chunk_index
   - Deposits: estimated challenge cost

2. Challenge window opens (~48 hours)
   - Provider must respond with chunk data + Merkle proofs
   - Cost split based on response time

3. Resolution
   - Valid proof: Challenge rejected, cost split by response speed
   - Invalid/no proof: Provider's full stake slashed
```

**Cost split by response time:**

| Response | Challenger pays | Provider pays |
|----------|-----------------|---------------|
| Block 1  | 90%             | 10%           |
| Blocks 2-5 | 80%           | 20%           |
| Blocks 6-20 | 70%          | 30%           |
| Blocks 21-100 | 60%        | 40%           |
| 100+ blocks | 50%          | 50%           |
| Timeout  | 0% (refunded + reward) | 100% (slashed) |

**Why this structure?**
- Provider always pays *something* when challenged (even if honest)—incentive to serve directly and avoid challenges entirely
- Fast responses minimize provider cost—incentive to respond promptly
- Challenger majority cost for honest provider—griefing is expensive
- Full slash on failure—catastrophic penalty deters cheating

### The Burn Option

At agreement end, the owner decides: pay the provider, or burn the locked payment.

**Why burn?**

Imagine a provider who technically kept the data but was slow, unresponsive, or hostile. Not slashable (data exists), but not satisfactory. Burning is a punishment signal:
- Provider gets nothing
- On-chain record of burn damages reputation
- Future clients see: "This provider had X agreements burned"

**Why would anyone burn?**

They've already lost the money either way—paying or burning, funds leave their account. The choice is: reward the provider, or punish them. For genuinely bad service, punishment is rational—it warns future clients and may improve the ecosystem.

### Freeloading Prevention

What stops a provider from storing nothing and fetching from other providers when challenged?

**Economics**: The freeloading provider risks their entire stake on other providers' reliability and cooperation. If the other providers also freeload, everyone gets slashed. If the other providers refuse to serve (why help a competitor?), the freeloader can't respond to challenges.

**Detection**: Freeloading adds latency. A provider fetching from elsewhere shows network delay; a provider reading from local disk shows disk latency. Clients measuring random read latency can detect and avoid freeloaders.

**Isolation mode** (future): Admin temporarily blocks providers B and C from serving, then challenges A. If A can't respond without fetching from B/C, A is caught.

### Collusion Resistance

What about multiple providers colluding to reduce physical redundancy or coordinate service degradation?

**Technical collusion (reducing storage):** Providers A, B, C coordinate—only A stores data, B and C proxy from A. This fails because:
- Latency measurements detect proxying (see [Latency-Based Selection](#latency-based-selection-and-geographic-redundancy))
- Each provider still needs full stake at risk
- Savings minimal (~$20/month) vs. risk (thousands in stake)

**Service degradation collusion (hostage scenario):** More subtle attack—providers store data but deliberately provide poor service to extort payments. They respond to challenges (avoiding slashing) but refuse normal reads unless paid extra.

Why this fails:
- **Client migration**: Poor service → clients pick different providers for future buckets
- **Reputation damage**: On-chain record shows provider has many non-renewed agreements
- **Limited leverage**: With 3+ providers, at least one likely defects to capture more business
- **Latency optimization**: Clients automatically shift traffic to responsive providers (see [Latency-Based Selection](#latency-based-selection-and-geographic-redundancy))
- **Challenge cost ceiling**: Can't charge more than challenge cost—client would just challenge instead

It's not binary (serve/don't serve). Providers can "barely serve"—responding only to challenges while degrading normal service. But this is self-defeating long-term as clients migrate to better providers, and short-term gains are capped by challenge costs.

**Organizational collusion (censorship):** Single entity runs providers globally, receives government pressure to censor. Protection through economics:
- Censoring all replicas = all stakes slashed (3 providers × 1000 DOT = ~$450,000)
- Pressure must exceed economic penalty to force compliance
- Permissionless replicas can't be controlled by original provider

We don't prevent collusion cryptographically. We make it economically irrational through stake requirements, practically difficult through latency-based verification, and strategically unstable through client optionality and provider competition.

---

## Proof-of-DOT and Read Incentives

Storage guarantees that data *exists*. But what makes providers *serve* it?

### Identity and Sybil Resistance

Before anything else, we need identity. Without it, reputation is meaningless, spam is free, and accountability is impossible.

Proof-of-DOT (detailed in the [Proof-of-DOT Infrastructure Strategy](https://docs.google.com/document/d/1fNv75FCEBFkFoG__s_Xu10UZd0QsGIE9AKnrouzz-U8/)) provides this foundation:

**For clients:**
- Lock DOT against a PeerID
- Providers can lookup PeerIDs on connection establishment
- Enables reputation: providers remember past interactions
- Prevents spam: creating identities costs money

**For providers:**
- Same Proof-of-DOT mechanism (sybil resistance, identity)
- Separately: providers lock collateral for storage agreements
- Collateral stake-per-byte ratio signals commitment level

### Payment Priority

Providers track cumulative payments received from each client. Clients who have paid more get priority in serving queues.

Example scheme: Providers serve paying clients the best they can (maybe even rank them, if resources get low). Non-paying new clients get also served well, but if they keep coming back, demanding service for free - priority degrades, incentivizing payments. Clients will thus spread the load amoung providers, but will eventually pay to maintain a good service for a resource they use regularly.

On viral content, where even the original visit can't be served well, because of load, the client software can suggest to the user to pay for faster access - because of huge demand. (Cents)

**The key distinction:** This is *payment history*, not stake amount. A client with 1 DOT staked who has paid 100 DOT over time gets better service than a client with 100 DOT staked who has never paid. Stake is about identity; payment history is about priority.

Distinction to proof of personhood: Proof-of-DOT is a weaker sybil resistance for network-level operations where fast sieving matters—quickly filter requests without investing resources. Proof of personhood protects scarce resources (votes, airdrops, free transactions); Proof-of-DOT protects cheap resources (read requests, connection slots). One person can have multiple identities if they pay—that's fine. In practice they complement each other: proven persons could get DOT for Proof-of-DOT registration for free.

### Why Providers Serve Data

Competition drives quality. The feedback loop is natural:

- Client feels mistreated? Switch providers or stop paying.
- Provider wants revenue? Treat paying clients well.

Clients connect to multiple providers, experience service quality directly, and vote with their feet. No complex monitoring required—just "did this work well for me?"

### Challenge as Price Ceiling

The challenge mechanism creates a ceiling that protects clients even against monopolistic providers.

If a provider demands more than the challenge cost to serve data, the rational client simply challenges on-chain and recovers the data via the proof. The provider:
- Gets no payment
- Pays challenge costs
- Loses reputation

So rational providers price *below* the challenge threshold. Most of the time, competition drives prices well below this ceiling anyway—it only matters for the edge case of a single provider attempting ransom - which should be avoided to begin with.

---

## Client Strategies

### Selecting Providers

Clients should evaluate providers on:

**Stake level**: Higher stake = more to lose = stronger incentive alignment. Match stake to data importance.

| Data importance | Example stake tier |
|-----------------|------------------------|
| Ephemeral (cache) | Any registered provider |
| Standard (backups) | Higher stake preferred |
| Critical (compliance) | Highest available stake |

*Note: Specific stake thresholds will emerge from market dynamics and can be refined based on production data.*

**Track record**: Check on-chain stats:
- Total agreements vs. agreements extended (extension = client satisfactionV2 - better structure

First draft)
- Agreements burned (burn = client dissatisfaction)
- Challenges received vs. failed (failed = catastrophic failure)
- Provider age (longer = more track record)

**Stake homogeneity**: Don't mix high-stake and low-stake providers for the same bucket. A 1000 DOT provider alongside a 10 DOT provider means the 10 DOT provider can safely freeload—they risk little while relying on the 1000 DOT provider.

### Latency-Based Selection and Geographic Redundancy

By tracking latency over time and shifting toward lower-latency providers, clients naturally sieve out freeloaders and slow providers. This happens automatically as part of normal usage.

**Why this works**: Physics doesn't lie. Cross-region latency is unavoidable—EU to US adds ~60-80ms round-trip minimum. A provider fetching from another region to serve you will always be slower than one serving from local storage. Over time, latency tracking reveals:
- Freeloaders proxying from other providers
- Slow or overloaded providers
- Providers not actually in their claimed region

**Geographic redundancy emerges**: If a client sees consistently low latency from certain providers in a regionand high latency from others, the low-latency providers are genuinely serving from Europe. By selecting providers with consistently good latency from different regions, you achieve verified geographic distribution—not by trusting claims, but by measuring physics.

**Cross-region verification in practice**:
1. Select providers in distinct regions (EU, US-East, Asia)
2. Know expected latency per region from your location
3. Measure actual latency via random chunk reads
4. Compare within regions—if one EU provider shows 100ms when others show 20ms, they're suspect
5. Over time, shift toward consistently fast providers per region

### Automated Verification

Client software should verify automatically, invisibly:

**On every normal use**:
- Directory browsing verifies directory chunks
- File opening verifies file chunks  
- Media streaming verifies sequential chunks

**Background sampling**:
- Weekly: 3 random chunks from random providers
- Flag latency anomalies or fetch failures
- Track per-provider reliability over time

**The result**: Verification becomes a byproduct of usage, not a conscious task. The lazy human problem is solved by disciplined software.

### When to Challenge

Challenges are expensive and adversarial. Use them when:
- Provider refuses to serve data off-chain
- Provider demands unreasonable prices
- You need on-chain proof of data availability
- You want to force recovery of specific chunks

Don't challenge for routine verification—that's what spot-checking is for. Challenge is the nuclear option when the provider has broken the social contract.

---

## Data Model

### Content-Addressed Chunks

All data is broken into fixed-size chunks (e.g., 256KB), each identified by its hash:

```
Chunk
├── hash: H256 = blake2_256(data)
├── data: bytes (up to 256KB)
```

Internal nodes in Merkle trees are also chunks—their content is child hashes:

```
Internal Node
├── hash: H256 = blake2_256(child_hashes)
├── children: [H256, H256, ...]
```

**Why content-addressed?**
- Deduplication: Identical chunks stored once
- Verification: Hash mismatch = corruption detected
- Cacheability: Any node can serve verified chunks

### MMR Commitments

Each bucket tracks state via a Merkle Mountain Range (MMR):

```
BucketSnapshot
├── mmr_root: H256       // root of the MMR
├── start_seq: u64       // first leaf sequence number
├── leaf_count: u64      // number of leaves
├── checkpoint_block: Block
├── primary_signers: BitVec  // which primaries signed
```

**Canonical range**: `[start_seq, start_seq + leaf_count)`

**MMR leaves** contain:
```
MmrLeaf
├── data_root: H256      // Merkle root of chunk tree
├── data_size: u64       // logical size of this data
├── total_size: u64      // cumulative unique bytes in bucket
```

**Append**: Add new leaf with new data_root
**Delete**: Increase start_seq (old leaves no longer in range)
**Freeze**: Lock start_seq—bucket becomes append-only forever

### Client-Controlled Layout

The protocol provides what is essentially a disk: content-addressed chunks of fixed size. Clients control layout completely.

Any filesystem technique works:
- Reserved chunks for metadata/directories (e.g., first chunk = root directory)
- Files referenced by byte offset + length
- Inodes, extent trees, FAT—whatever the application needs
- Encryption of all content including directory structure

**Example layout:**

```
Chunk 0-2: [encrypted directory structure]
Chunk 3-10: [encrypted file: photo1.jpg]
Chunk 11-15: [encrypted file: document.pdf]
...
```

The client reserves the first chunks for directory structure. With large chunk sizes (e.g., 256KB), multiple directory levels fit in a single chunk. The client fetches chunk 0, decrypts directory entries, learns where files live (by byte offset + length), and fetches. The provider sees only "client requested chunks 0, 3-10"—no semantic meaning.

**Alternative: one file per leaf.** A chat channel might store each media file as its own MMR leaf—the leaf's `data_root` is simply the Merkle root of that file's chunks. No filesystem structure needed; the chat protocol tracks which leaf corresponds to which message.

**Privacy by design**: Providers see only encrypted bytes. They learn nothing about file structure, metadata, or content. The application layer—entirely client-controlled—imposes meaning on the chunks.

---

## Use Cases

### Personal Backup

```
Setup:
├── Create bucket (single admin)
├── Add 2-3 diverse providers
├── Encrypt locally with master key

Operation:
├── Incremental backup: content-defined chunking
├── Deduplication: unchanged chunks already exist
├── Spot-check: 3 random chunks weekly (automated)

Recovery:
├── Fetch from any provider (all have full copy)
├── Verify via hash, decrypt locally
```

### Media in Chat

```
Setup:
├── Creator makes bucket
├── Add members as Writers
├── 1-2 providers (low redundancy OK for ephemeral)

Operation:
├── Member uploads image → gets data_root
├── Message contains: {bucket_id, data_root, leaf_index}
├── Recipient fetches directly from provider

Concurrent writes:
├── Content-addressed: parallel uploads don't conflict
├── Commits coordinated via chat message ordering
```

### Public Website

```
Setup:
├── Public bucket (anyone can read)
├── Geographically diverse providers
├── Frozen (append-only for version history)

Discovery:
├── DNS TXT record: bucket_id + current leaf_index
├── Client picks fastest provider for their region

Updates:
├── New content → new MMR leaf
├── Update DNS to new leaf_index
├── Old versions remain accessible via old leaf_index
```

### Business Compliance Archive

```
Setup:
├── min_providers = 3
├── Frozen bucket (immutable audit trail)
├── 3-5 providers (stake homogeneity, infrastructure diversity)

Verification:
├── Continuous background sampling
├── Challenge on any anomaly
├── On-chain checkpoints = timestamped proof

Compliance:
├── Frozen = deletions impossible
├── Checkpoints = provider acknowledgments recorded
├── Slashing = accountability for data loss
```

---

## Comparison with Existing Solutions

### vs. Filecoin

| Aspect | Filecoin | This Design |
|--------|----------|-------------|
| Proof mechanism | Cryptographic (PoRep/PoSt) | Game-theoretic (challenges) |
| Chain load | Heavy (every 24h per sector) | Minimal (disputes only) |
| Write latency | Minutes-hours (sealing) | Sub-second |
| Hardware | GPU required | Commodity |
| Sector size | 32GB fixed | Flexible (any size) |
| Best for | Cold archival | Hot interactive |

**Trade-off**: Filecoin provides stronger cryptographic guarantees and objective guarantees even for data no one is actively using. We optimize for data someone cares about and verifies.

### vs. IPFS

| Aspect | IPFS | This Design |
|--------|------|-------------|
| Discovery | DHT (2-10s, unreliable) | Chain (instant, reliable) |
| Persistence | No guarantees | Contractual + slashing |
| Read incentives | None | Proof-of-DOT priority |
| Mutable references | No | Yes (bucket-addressed) |

**Trade-off**: IPFS provides content addressing without guarantees. We add economic guarantees at the cost of requiring on-chain bucket setup.

### vs. Arweave

| Aspect | Arweave | This Design |
|--------|---------|-------------|
| Model | Permanent, endowment | Contractual, renewable |
| Payment | One-time upfront | Ongoing agreements |
| Guarantees | "Forever" | Until agreement expires |
| Flexibility | Write-once | Mutable (unless frozen) |

**Trade-off**: Arweave optimizes for permanence with upfront payment. We optimize for flexibility with ongoing relationships.

---

## Rollout

### Phase 1: Buckets and Basic Storage

Deploy bucket infrastructure and storage agreements. Ecosystem providers offer initial storage (free or low-cost).

**Establishes:**
- On-chain discovery (bucket → agreements → providers)
- Working protocol implementation
- Initial application developers

### Phase 2: Challenges and Guarantees

Add challenge mechanism. Providers must stake. Clients can challenge and slash.

**Establishes:**
- Economic guarantees beyond reputation
- Slashing for misbehavior
- Trust model for critical data

### Phase 3: Proof-of-DOT

Add DOT staking for sybil resistance and read priority. Payment history tracking enables quality differentiation.

**Establishes:**
- Identity layer
- Quality-of-service tiers
- Foundation for provider competition

### Phase 4: Third-Party Providers and Replicas

Open to third-party providers. Add permissionless replica agreements.

**Establishes:**
- True decentralization (permissionless participation)
- Provider competition on price and quality
- Redundancy beyond admin-controlled primaries

**Why this works:** Each phase is functional standalone. No bootstrap paradox (need users for providers, need providers for users). The system works at every stage—it just gets better.

---

## Future Directions

### Isolation Mode

Admins can instruct providers to temporarily refuse serving non-members, then challenge a specific provider. If that provider was freeloading (fetching from others), they can't respond. Detects freeloading without on-chain enforcement. Note: This was explained in more detail in the previous version of this doc, but became harder with the introduction of replicas - which should not be controllable by the admin. Incentives still align as honest providers have an interest to help catching free-loaders. Latency measurements and high stake should get us very far though.

---

## Summary

We've designed a storage system for the common case: data that someone cares about.

**The key insight:** When clients have skin in the game—they paid for storage—they naturally verify. Verification becomes automatic, invisible, a byproduct of normal use. The "verifier's dilemma" disappears.

**The architecture:** Buckets provide stable identity. Providers lock stake and face slashing. Primaries handle writes; replicas add permissionless redundancy. The chain is the credible threat that keeps everyone honest without being the hot path.

**The result:** Storage that scales with provider capacity, not chain throughput. Writes are instant. Reads are fast. Guarantees are economic, not cryptographic—and for active data with interested clients, that's enough.

For data no one ever checks, no one ever reads, no one ever wants or fire & forget use cases or needed stronger objective guarantees, even for unpopular data—use Filecoin.

---

## References

### Filecoin

1. **Sealing and Sector Sizes**: Filecoin uses 32GB and 64GB sectors. Sealing typically takes 1.5-3 hours with GPU acceleration.
   - [Storage Proving | Filecoin Docs](https://docs.filecoin.io/storage-providers/filecoin-economics/storage-proving)

2. **WindowPoSt (24-hour proof cycle)**: Every sector is proven once per 24-hour proving period, divided into 48 deadlines of 30 minutes each.
   - [PoSt | Filecoin Spec](https://spec.filecoin.io/algorithms/pos/post/)
   - [What's Window PoST? | Trapdoor Tech](https://trapdoortech.medium.com/filecoin-whats-window-post-7361bfbad755)

3. **Proof of Data Possession (PDP)**: Launched May 2025, enabling hot storage verification without sealing.
   - [Introducing PDP: Verifiable Hot Storage on Filecoin](https://filecoin.io/blog/posts/introducing-proof-of-data-possession-pdp-verifiable-hot-storage-on-filecoin/)

### IPFS

4. **DHT Lookup Latency**: Median retrieval times of 2.7-4.4 seconds; P90/P95 can extend to 10+ seconds.
   - [Design and Evaluation of IPFS: A Storage Layer for the Decentralized Web](https://arxiv.org/pdf/2208.05877)
   - [IPFS KPIs | ProbeLab](https://www.probelab.io/ipfs/kpi/)
   - [Consensys IPFS Lookup Measurement](https://github.com/Consensys/ipfs-lookup-measurement)

### Network Latency

5. **Transatlantic Latency**: Round-trip times between EU and US hubs typically range 60-80ms, with theoretical minimum ~55ms based on speed of light in fiber.
   - Physical constraint: ~5,500km distance, light travels at ~200,000 km/s in fiber

### Detection Probability

6. **Spot-check Math**: For as little as 3 random checks per week with 10% data deletion:
   - P(miss per week) = 0.9³ = 0.729
   - P(detect in 13 weeks) = 1 - 0.729¹³ ≈ 0.98
   - P(detect in 26 weeks) = 1 - 0.729²⁶ ≈ 0.9997
