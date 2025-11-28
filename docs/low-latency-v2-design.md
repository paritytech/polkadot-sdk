# Low-Latency Parachains v2

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Review |
| **Version** | 1 |
| **Related Designs** | [Speculative Messaging](speculative-messaging-design.md) |

---

## Table of Contents

1. [Introduction](#introduction)
2. [Status Quo](#status-quo)
3. [Goals](#goals)
4. [Non-Goals](#non-goals)
5. [Solution Overview](#solution-overview)
6. [Basic Relay Chain Fixes](#basic-relay-chain-fixes)
7. [Scheduling Parent](#scheduling-parent)
8. [Acknowledgements](#acknowledgements)
9. [Speculative Messaging](#speculative-messaging)
10. [Implementation Details](#implementation-details)
11. [Threats](#threats)
12. [Research](#research)
13. [Conclusion](#conclusion)

---

## Introduction

For many use cases that go beyond pure speculation on price on exchanges, we
have humans more or less directly interacting with the blockchain and here
especially for more complex usage patterns (not just a single transfer), latency
matters, e.g. highly interactive and dynamic use cases. In particular for
situations where the success of the user initiated transaction depends on what
others are doing. Consider DeFi or any form of bidding, where success is by no
means guaranteed and depends on other bids coming in. For these scenarios we
actually need fast confirmation that a transaction was sequenced, achieved a
particular result with respect to that sequence and won't be replaced by another
sequence. This implies having built an actual block, forming a canonical chain:
No forks!

Super-low 1-block latency means we need to have a form of early finality
guarantees that are somewhat enforced. Otherwise we are entirely relying on
single actors being honest, which completely defeats our mission.

As a historical example, let's consider Bitcoin. It does allow forks, but they
are expensive—like really expensive! Hence if someone created a fork, just to
fool you, the gain to the block author from this fraud attempt would need to
outweigh the cost for creating the fork. Users can then reason that they are
very likely not cheated, as the cheater would lose money doing so. The
confidence grows with time, as costs grow with the length of the fork. This is a
common theme in blockchains (including Polkadot): Confidence grows with time
passed, which directly works against our low-latency plans: We need to adjust.

---

## Status Quo

As of now, for confirming a user transaction with confidence, we heavily depend
on all of parachain consensus to have happened and the transaction to have been
finalized by the relay chain. Why is that and what problems are we facing?

### Parachain

A produced parachain block is not much of a guarantee to anything. The first
problem is that a collator is completely free to equivocate right now. So a
single collator can, without any consequences, produce one block, show it to you
and submit another block to the relay chain. If you base any decisions on that
block that was shown to you (e.g. hand out some item in a shop), the collator
caused a double-spend.

The next problem is that parachain blocks are based on relay chain blocks, the
so-called relay parent of the block. This comes with two issues:

1. We might pick a relay parent of a relay chain fork that will not become
   canonical: We picked the wrong fork. In that case the produced block will no
   longer be accepted by the relay chain and needs to be discarded.
2. The relay chain only allows a window of relay parents. If the relay parent is
   too old, the block also will no longer be accepted. If the collator or the
   network has any issue, it might not be able to submit the collation in time.
   Another reason would be a censoring or malfunctioning backing group. And even
   if the submission to the backing group happened in a timely fashion, that's
   not enough: The full backing and inclusion pipeline needs to succeed before
   the relay parent goes out of scope.

### Relay Chain

The relay chain gives us two kinds of confirmations:

1. It confirms the validity of a block. This way a user does not need to run a
   full node to be sure that a block is valid.
2. Even more importantly: It eventually guarantees that the block is canonical
   and won't be replaced with an alternative.

The relay chain has block times of 6s. Getting a parachain block included takes
2 relay chain blocks. Together with the block building ahead of time
(asynchronous backing), this results in a total latency of 18s, for any
transaction that aims to be secured by Polkadot's economic security. At this
stage we have a strong guarantee on the validity of the block, because if it
were invalid, approval checkers would now initiate a dispute and the backing
validators would lose all their stake—but we still have no (strong) guarantee on
canonicality.

```
Current Parachain Block Confirmation Timeline
═══════════════════════════════════════════════

t=0s      Parachain block building (async backing)
          │
t=6s      │  Relay chain block N
          │  └─> Candidate backed
          ▼
t=12s     Relay chain block N+1
          └─> Candidate included (availability distributed)
          └─> ✓ Validity almost certain (significant slashes if not)
          ✗ NOT canonical yet (relay chain can fork/reorg)
          
          ... time passes ...
          
t=36s+    ✓ Relay chain finality
          ✓ Block is canonical and won't be reverted
```

The relay chain can experience forks and reorgs and a previously valid block can
get reverted, resulting in collators having to rebuild their blocks (relay
parent either went out of scope or is on a different fork), potentially arriving
at a different (also valid) sequence. Things that go wrong in practice:

1. Connectivity issues (Collator - Validator and Validator - Validator)
2. Legitimate systematic relay chain forks (AURA-BABE)
3. Relay chain block producers not producing blocks or not putting candidates in
   (Bad validators)
4. Dropping of candidates at session boundaries (A limitation of the current
   implementation)
5. Dropping of candidates due to weight (e.g. containing a heavy runtime
   upgrade)
6. Availability taking longer than a single block—keeping the core occupied,
   preventing the next candidate from getting backed

Therefore as of now, the probability of a parachain block becoming
final/canonical increases over time, but for any strong guarantee you actually
need to wait for relay chain finality. Which takes end-to-end in the ballpark of
36s.

### Full List of Block Confidence Issues

#### Normal Operation

Roughly in the order of severity/likelihood:

- **Connectivity Issues**: Collators having trouble connecting to the backing
  group
- **Relay chain forks**
- **Overlapping parachain slots**
- **Session Boundaries**: The relay chain is dropping candidates that are still
  pending availability at session boundaries
- **Relay chain block going overweight** (candidates get dropped)—most likely to
  happen on parachain runtime upgrades
- **Configuration Changes**: 
  1. Very rare
  2. Only causing problems, if the new configuration is not compatible with the
     old one
- **Parachain runtime upgrades**: Upgrade restriction signal is not properly
  handled.

#### Malicious Actors

- **Collator Equivocation**—no consequences
- **Backing group signing invalid blocks**—full slash of backers
- **Backing group censoring** (blocks go out of scope)—less rewards for backers
- **Relay chain block author censoring the parachain**—on purpose not backing
  some candidates on chain

### Problem Summary

| Problem | Impact | Severity | Status |
|---------|--------|----------|--------|
| **Relay Chain Forks** | Blocks become obsolete | High | Inherent to current design |
| **Collator Equivocation** | Double-spend attacks possible | High | No consequences currently |
| **Session Boundaries** | Blocks dropped if pending at session change | High | Implementation issue (needs fix) |
| **Relay Parent Expiry** | Blocks discarded if not included quickly | Medium | Limited validity window |
| **Configuration Changes** | Blocks rejected due to config mismatch | Low | Rare, needs fix for edge cases |
| **Runtime Upgrade Restriction** | Signal timing issues | Low | Edge case, needs fix |

---

## Goals

We would like to achieve:

1. **Much lower latency**. Instead of 36s, ideally we could get at least
   somewhat secured confirmations within subseconds.

2. **No single trusted party**. We won't depend on a single party to be honest.
   No centralized "trusted" sequencer. Misbehavior cannot be prevented, but
   requires collusion between collators and will be punished. Low-latency
   confirmations won't be as strong as relay chain finality, but will still
   derive some security from the relay chain: While the relay chain cannot
   prevent any misbehavior, we will utilize it to ensure misbehavior will be
   avenged, assuming at least one honest and live collator exists.

3. **Opt-in add-on**. Low-latency confirmations are an opt-in add-on, not
   affecting our current trust model for any user who chooses to rely on relay
   chain finality. In particular censorship resistance and liveness properties
   should be maintained.

4. **High confidence in practice**. Low-latency should have high confidence in
   practice even with malicious actors. While you should still wait for relay
   chain finality for any high-value transactions, the day-to-day should work
   with low-latency. We aim to be able to get collators slashed: So in essence
   any transaction with a value in the ballpark/lower than collator stake should
   be economically secure at low-latency.

---

## Non-Goals

Compete with ultra-low latency (<<100ms) and extreme global consistency
requirements for global trading: e.g. what Solana is aiming for. These require a
good amount of centralization and depend 100% on vertical scaling as opposed to
horizontal scaling. Polkadot was built to be decentralized and to scale
horizontally. Therefore it does not make sense to try to compete in an area,
Polkadot was never designed for. Instead we should focus on use cases where
these other blockchains are inferior to Polkadot. In particular: Horizontal
scaling for example is much cheaper, as instead of requiring every node in the
network to be able to handle all the ever increasing (vertically scaled)
traffic, state and load, we spread the load among nodes. What Polkadot enables
is that even low-powered machines can participate and seamlessly communicate
with a network handling hundreds of thousands to millions of transactions per
second.

That being said, this document is laying the groundwork of what would be
necessary to compete. By putting block confidence in the hand of the collator,
one can have super fast trusted acknowledgments, with the assurance that your
transactions will make it, as long as the collator is honest. The benefit of the
Polkadot solution is that we would still have an interconnected chain with the
rest of the ecosystem and communication with that ecosystem being actually
trustless still. (Assuming the runtime is at least managed in a decentralized
fashion.)

Throughput figures can be reached by the means of elastic scaling and by
building super chains (interconnected chains, with messaging latency between
them in the ballpark of milliseconds), based on [speculative
messaging](speculative-messaging-design.md).

---

## Solution Overview

### Requirements

We aim for "collator based" finality, meaning that we want collators to commit
to a canonical chain, long before the relay chain would canonicalize any fork.
To do this, we are going to introduce acknowledgement signatures and will
introduce the necessary relay chain decoupling so collators can actually commit
to a canonical fork, as they stay in control of the validity of their blocks:

1. Parachains need to become immune to relay chain forks. Otherwise it is
   impossible to commit to a canonical chain ahead of time.
2. Relay parents and thus parachain blocks must stay valid and not become
   obsolete quickly due to relay chain advancements or other reasons.

In other words, if we want collators to strongly commit to a chain becoming
canonical, we also need to give them the necessary ownership. Within some
realistic boundaries, if an acknowledged block does not get finalized, we need
to be able to blame and punish the collators: No excuses!

### Solution Ingredients

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                    Current System (Status Quo)                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Parachain Block                                                                │
│  ┌──────────────────────────────────────────────────────────────────────┐       │
│  │ Relay Parent: Block N (recent, can become invalid)                   │       │
│  │   ├─> Execution context (messages, config, etc.)                     │       │
│  │   └─> Scheduling context (backing group, core, secondary checkers)   │       │
│  │                                                                      │       │
│  │ Issues:                                                              │       │
│  │   • Forks invalidate blocks                                          │       │
│  │   • Session boundaries drop blocks                                   │       │
│  │   • No collator accountability                                       │       │
│  │   • 36s wait for finality                                            │       │
│  └──────────────────────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────────────────────┐
│                    Low-Latency v2 System                                      │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  Parachain Block                                                              │
│  ┌───────────────────────────────────────────────────────────────────┐        │
│  │ Relay Parent: Block N (can be old/finalized)                      │        │
│  │   └─> Execution context only (messages, config, etc.)             │        │
│  │                                                                   │        │
│  │ + Scheduling Parent: Block M (recent leaf)                        │        │
│  │   └─> Scheduling context (backing group, core, secondary checkers)│        │
│  │                                                                   │        │
│  │ + Acknowledgement Signatures                                      │        │
│  │   └─> Collators commit to canonical chain                         │        │
│  │                                                                   │        │
│  │ Benefits:                                                         │        │
│  │   ✓ Immune to relay chain forks                                   │        │
│  │   ✓ Session boundaries don't invalidate blocks                    │        │
│  │   ✓ Collators accountable (slashable)                             │        │
│  │   ✓ Sub-second confidence possible                                │        │
│  └───────────────────────────────────────────────────────────────────┘        │
└───────────────────────────────────────────────────────────────────────────────┘
```

Lowest hanging fruits first: We will fix current implementation shortcomings
that affect validity of parachain blocks for no good reason. In particular we
need to fix:

- Crucial: Candidates (and thus parachain blocks) becoming invalid at session
  boundaries
- Edge Case: Parachain runtime upgrade restriction signal fix
- Edge Case:Configuration upgrades invalidate candidates

Next we need to ensure that parachain blocks stay valid, even in the event of
relay chain forks and stay valid longer so network problems, censoring backers
or poorly performing validators cannot easily cause parachain blocks to become
invalid. For this we will introduce an additional concept called **scheduling
parent** in addition to the existing relay parent of a candidate, with this we
will be able to:

- **Preserve validity**: Adjust the relay chain to accept candidates with much
  older relay parents
- **Ensure submission**: Enable resubmission of an unaltered parachain block in
  a new candidate by the next collator

With the above, we put collators in control and thus we laid the groundwork for
the final ingredient: **Acknowledgment signatures**. With these, collators
commit to blocks becoming canonical (and thus submitted to the relay chain).
They will be rewarded for timely acknowledgements and punished for acknowledged
blocks that do not become canonical.

### Tradeoffs: Speculative Messaging

The relay chain decoupling, in particular building on older/finalized relay
chain parents, comes at the cost of increased messaging latency, as the relay
parent provides the context for message receival. To mitigate this drawback we
are also introducing a new mechanism replacing on-chain messaging, which is
[Speculative Messaging](speculative-messaging-design.md).

Speculative messaging is able to restore the messaging latency we had with
building on the most recent relay parent and in many cases we will be able to
significantly improve upon it: For chains under the same trust domain, we can
bring messaging latency down to be even lower than parachain block times!

---

## Alternatives Considered

### Inclusion Lists (Transaction Ordering Without Execution)

An alternative approach to achieving low-latency confirmations would be to adopt
an inclusion list (IL) mechanism, similar to proposals in the Ethereum ecosystem
(e.g., [FOCIL](https://meetfocil.eth.limo/)). Instead of waiting for collators
to build and execute full blocks, this approach would have collators order
transactions without executing them, potentially reducing latency by skipping
the execution step.

#### Comparison

| Dimension | Acknowledgements (Chosen) | Inclusion Lists |
|-----------|---------------------------|-----------------|
| **Latency** | ~100ms (achievable block time) | Potentially <100ms |
| **Result Certainty** | Known after execution | Unknown (transaction may panic/fail) |
| **Ordering Guarantee** | Strongly enforced (slashable) | Weakly enforced (limited punishment) |
| **Censorship Resistance** | Maintained (1 honest collator) | Compromised (signatures in block) or weak (collator voting) |
| **Threat Model** | 1 honest collator assumption | Super-majority honest assumption |
| **Punishment for Misbehavior** | Full stake slash | Block rewards only |
| **MEV Resistance** | Better (strong enforcement) | Worse (collusion possible, limited punishment) |
| **Complexity** | Moderate | High ("blocks" on top of blocks) |
| **Synergy with Basti Blocks** | Excellent (makes them valuable) | Poor (renders Basti blocks redundant) |

#### Why Inclusion Lists Were Not Chosen

While inclusion lists could theoretically provide marginally lower latency, they
come with significant tradeoffs that make them unsuitable for Polkadot's
parachain model:

**1. Weaker Guarantees**

- A transaction being listed does not guarantee it will be successfully included
  in the final block
- The execution result is unknown at confirmation time (transaction may panic or
  fail)
- Panicking transactions create ordering complications and reduce usefulness
- Users cannot rely on the outcome when making time-sensitive decisions

**2. Security Model Degradation**

The most critical issue is that inclusion lists severely weaken Polkadot's
security model:

- **Cannot enforce via relay chain fork choice**: Unlike Ethereum, where the
  beacon chain can enforce inclusion lists through fork choice rules, Polkadot's
  relay chain decides forks independently of parachain promises. This leaves us
  with two bad options:
  
  - **Option A - Signatures in blocks**: Require IL signatures in parachain
    blocks to enable relay chain enforcement. This completely sacrifices
    censorship resistance, as now collators must cooperate for successful block
    submission.
  
  - **Option B - No enforcement**: Don't put signatures in blocks. This means we
    can only punish misbehavior, not prevent it. But punishment requires
    provable misbehavior, which cannot be proven in the runtime for IL
    violations. This forces reliance on collator voting mechanisms.

- **Super-majority honest assumption required**: Without relay chain
  enforcement, we must rely on collator voting to identify and punish
  misbehavior. This replaces the current **1 honest collator** assumption with a
  **super-majority honest** assumption—a significant security downgrade.

- **Limited punishment capability**: Even if we detect misbehavior, punishment
  is severely limited. A collator can always avoid fulfilling an IL promise by
  simply not producing a block. Therefore, we can only take away block rewards,
  not slash stake. This means MEV extraction through collusion remains
  profitable despite "punishment." (assuming colluding collators).

**3. Marginal Latency Gains**

The latency improvement offered by inclusion lists is marginal in practice:

- **100ms blocks are achievable**: With optimizations to execution speed, we can
  already achieve ~100ms block times with the acknowledgement approach
- **Network latency floor**: For any decentralized setup, we're already
  approaching network latency limits. Further reductions require centralization.
- **Transaction streaming**: With transaction streaming, effective latency with
  100ms blocks is already around ~100ms.
- **Better alternatives exist**: If even lower latency is needed, better
  improvements come from:
  - Execution speed optimizations (NOMT, batch signature verification, parallel
    execution)
  - Reducing constant per-block overhead
  - Dynamic block times (making very low block times more feasible)

**4. Architectural Complexity**

Inclusion lists add "blocks on top of blocks"—transaction ordering separate from
block execution. This increases system complexity while providing weaker
guarantees than the simpler acknowledgement-based approach.

#### Note on Ethereum's Approach and Recent Developments

Recent research on Ethereum's [FOCIL (Fork-Choice enforced Inclusion
Lists)](https://eips.ethereum.org/EIPS/eip-7805) reveals important insights:

**FOCIL's Design:**
- Uses a **committee of validators** (not a single proposer) to construct
  inclusion lists
- Incorporates force-inclusion into the **fork-choice rule** itself
- Attesters only vote for blocks that include transactions from the IL committee
- Introduced in
  [EIP-7805](https://ethresear.ch/t/fork-choice-enforced-inclusion-lists-focil-a-simple-committee-based-inclusion-list-proposal/19870)
  and actively being developed for Ethereum

**Why FOCIL Works for Ethereum but Not Parachains:**

1. **Fork choice control**: Ethereum's beacon chain can enforce inclusion lists
   through fork choice because the beacon chain IS the source of truth for
   forks. Polkadot's relay chain decides forks independently of parachain
   promises—we cannot leverage fork choice the same way.

2. **Committee size**: Ethereum's large and highly staked validator set (>1
   million validators) makes committee-based approaches practical and secure.
   Parachains have much smaller collator sets, making super-majority assumptions
   problematic.

3. **Purpose alignment**: FOCIL is primarily designed for **censorship
resistance** against MEV extraction and builder centralization—not for
low-latency confirmations. As noted in the [FOCIL
documentation](https://meetfocil.eth.limo/), "unconditional ILs can offer better
short-term censorship resistance, they might also be easier to crowd out because
they might be used to offer products like preconfirmations."

4. **Enforcement timing**: Even with FOCIL, enforcement happens through
   conditional attestation and subsequent fork choice, not at the time of
   receiving the IL. Attesters only vote for blocks satisfying IL conditions,
   and fork choice then selects the canonical chain based on these attestations.
   Users must still wait for this process to complete to have strong guarantees,
   not just the IL itself.

5. **Conditional inclusion**: FOCIL adopts conditional inclusion, accepting
   blocks that may lack some IL transactions if they cannot append them or if
   blocks are full. This further weakens guarantees for specific transaction
   inclusion.

**Research on Latency-Throughput Tradeoffs:**

Recent [research on blockchain latency and
throughput](https://www.paradigm.xyz/2022/07/consensus-throughput) confirms that
latency and throughput are typically a tradeoff, with an inflection point where
latency increases sharply as system load approaches maximum throughput. For
systems aiming for low latency, execution speed optimizations (as proposed in
this design) are more effective than adding ordering layers on top of execution.

#### Synergy with Basti Blocks

The acknowledgement-based approach has an important synergy with Basti blocks
(multiple blocks per PoV):

- Acknowledgements provide reliable low-latency confirmations for blocks
- This makes Basti blocks practical and valuable for real-world use cases
- With Basti blocks we can have very small blocks (100ms seems possible) -
  leading to fast block based confirmations
- In contrast, the inclusion list approach would render Basti blocks redundant,
  as you'd be relying on ordering of transactions for confirmation rather than
  blocks

#### Conclusion

While inclusion lists could theoretically provide marginally lower latency
(perhaps <100ms vs ~100ms), they:

1. Provide much weaker guarantees (no execution result, no ordering enforcement)
2. Severely degrade the security model (super-majority honest vs 1 honest
   collator)
3. Limit punishment to block rewards (vs full stake slashing)
4. Add significant complexity (transaction ordering layer on top of block
   execution)

The acknowledgement-based approach provides:

- Strong guarantees: executed blocks with known results and enforced canonical
  ordering
- Strong security: 1 honest collator assumption with full stake slashing for
  misbehavior
- Practical low latency: ~100ms blocks are achievable
- Better path forward: synergizes with execution optimizations and Basti blocks

For a decentralized system with economic security guarantees, the
acknowledgement approach seems to be the better choice.

---

## Basic Relay Chain Fixes

As of today there exist some basic issues, which prevent collators from being
able to take ownership on block confidence. These issues stem merely from the
fact that during initial development, block confidence and low-latency was not a
concern and thus block confidence issues had been considered harmless. This was
perfectly fine reasoning at the time, but now we have a different set of
requirements and we need to adjust. Luckily the necessary fixes are rather
straightforward.

### Session Changes

In the current implementation, the relay chain clears out any cores that are
still occupied on a session boundary. Which means that a candidate that got
backed in the last block of a session will be discarded and needs to be rebuilt.

The proposed fix is to virtually extend the session for cores that are still
occupied and let them get freed in the old session. With this we will then
maintain an old assumption slightly adjusted: The assumption was that we can
determine the session a candidate was backed in, by the session of the relay
parent—they were forced to be the same. This will no longer be true, because we
aim to support very old relay parents. Instead we will ensure a new invariant,
which is that the scheduling parent's session will match: Therefore we can
lookup the provided scheduling session to determine the responsible validator
set.

### Parachain Runtime Upgrades

#### UpgradeGoAhead Signal

This signal is already handled properly, no changes are needed.

#### UpgradeRestrictionSignal

This one is tricky, it is used to signal to the parachain that a runtime upgrade
would not be accepted at the time, thus preventing the parachain runtime from
producing a block that would get discarded.

The issue with this signal is that it is broken already with asynchronous
backing and even more so with elastic scaling: The signal will be in effect
before the parachain runtime becomes aware of it, because of the older relay
parent we are using.

**Solution**:

1. Parachain runtime assumes that upgrades are illegal after having requested an
   upgrade, until we either see the GoAhead signal or an error signal
2. Unchanged: We also assume upgrades are illegal if the
   `UpgradeRestrictionSignal` is present. But this only becomes relevant, after
   the GoAhead or an error signal is received
3. We fix the relay chain to always tell something—always. Code paths must come
   with an error signal (to be introduced)

### Configuration Changes

For checking a candidate, we plainly retrieve the currently active
configuration. If the parachain block that is being checked is based on an older
relay parent, it might have been built in the context of an older
configuration—potentially causing the candidate to get rejected by the relay
chain.

As of now configuration changes are session buffered, therefore the issue is
masked by the before mentioned issue of dropping candidates on session
boundaries. But once that is resolved, we need to provide a solution here too.

**Solution**: Configuration is global — we cannot just delay the configuration
change for everybody. Instead we need to keep old configurations around and
validate candidates based on the configuration of the relay parent's session.

Concretely, in the relay chain runtime, we will keep the last n configurations,
where n is large enough to cover the oldest supported relay parent block. Then
we will not check candidates against the current configuration, but against the
configuration of the session of the relay parent—as this is the configuration
the candidate block has been operating under.

### Relay Chain Context

The relay parent provides a context for the execution of the parachain block,
essentially providing information about the outside blockchain world. It is part
of the means of how parachains interact with the broader Polkadot ecosystem.

Concretely it gives access to the following via relay chain storage proofs:

1. **Time/Slot information**
2. **Messages**
3. **Randomness**
4. **Configuration**
5. **Runtime upgrade go ahead signal**
6. **Runtime upgrade restriction signal**
7. **Authorities**

We aim to be able to use older relay parents in order to not be affected by
relay chain forks. Let's examine feasibility:

#### Time/Slot Information

This is used for:

1. Determining the parachain slot and thus the eligible parachain block producer
2. It determines who is allowed to provide scheduling information (which core
   and which backing group to use and which depth in the claim queue)

This is actually the most interesting part when it comes to low-latency and we
will look much more closely into this in the scheduling parent section.

#### Messages

This is the one thing we get from the relay chain context which is the most problematic with regards to relay parent age. If we build on finalized relay chain blocks, this means we are adding >18s to the messaging latency.

This is the biggest downside of using older relay parents and needs to be mitigated for the solution to be practical. This is why this design should go hand-in-hand with the introduction of [speculative messaging](speculative-messaging-design.md).

#### Randomness

The most useful randomness is randomness that is not known ahead of time (before
building the block), but we don't have this. Randomness is already known ahead
of time, with older relay parents it is worst case known a little more ahead of
time. For old blocks that just failed to be submitted (the case where relay
parents can become the oldest), this seems even less problematic as yes the
randomness was known for a while, but also the block that used it existed for a
while—so from the perspective of the block it was not old at all.

#### Configuration

With the relay chain fixes in place, configuration changes don't strongly affect
the maximum relay parent age. Configuration changes require governance action on
the relay chain, which takes days (at least) already. If we allowed relay
parents to be a day old (which is plenty), then worst case we delayed a
configuration change by an additional day in practice.

#### Runtime Upgrade Signals

This signal is handled perfectly already.

#### Conclusion

So far there is not much stopping us from allowing older relay parents and thus
laying the basis for high block confidence. The tricky part is time/slot
information, which requires the introduction of a scheduling parent.

---

## Scheduling Parent

### Motivation

If a block does not make it on the relay chain at the time where it was supposed
to, it is lost. In practice the relay chain has a bit of lenience and blocks can
still make it in the next block—assuming that the parachain still has the exact
same core. This lenience makes the problem less severe, but it does not solve
it. If you missed two opportunities, the block is still lost; if the parachain
core changes or is no longer available (on-demand/core sharing), the block is
lost.

Reasons for a block not making it according to schedule are numerous: Collation
did not reach the block author in time (networking), the block was full (runtime
upgrades), availability took longer and core did not get free in time, backers
are censoring or non-functional, …

Long story short, discarding a block because it did not make it according to
schedule is not an option if we aim to give collators control on block
confidence.

### Scheduling Context

The reason we have to discard blocks if they don't make it according to schedule
is because in addition to providing the relay chain context to a parachain
block, the relay parent also serves an additional very much unrelated purpose:
It provides a **scheduling context**.

Concretely, we use it to select the responsible backing group.

Using the relay parent here made a lot of sense in the beginning, as with
synchronous backing only the most current leaf could be selected as the relay
parent of a block, thus we had a pretty unambiguous selection of the backing
group.

With asynchronous backing, where we allow older relay parents, this already
became ambiguous. At group rotation boundaries, we no longer have a unique
selection, as collators are allowed to choose from a selection of relay parents.
A non-ideal situation and already suggests that we are conflating concerns,
where we should not. It does get worse though, if we allow much older relay
parents. Then we are essentially letting collators pick their backing group
freely—which is unintended, ruins the schedule and is also an attack vector, as
we cannot even force collators to only pick one.

There is also a second, smaller problem: Since elastic scaling, the mapping core
↔ backing group is no longer enough information for scheduling. We also need the
candidate to communicate which core it is intended for, as the parachain can now
be scheduled on more than one core (see RFC-103).

### Basic Idea

We introduce a second relay parent, which provides a separate context—the
**scheduling context**. It is used on the parachain and the relay chain for
everything related to scheduling (essentially assigning validators to check the
block). This is:

- What core to use at what block
- Determining the responsible backing group
- Determine responsible approval checkers and validators for disputes

This information is very transient. Responsible backers rotate all the time,
responsible secondary checkers rotate at session boundaries. That is why it
makes sense to not tie these to the validity of a parachain block, otherwise we
are enforcing a very limited lifetime.

```
Relay Chain Blocks:
... ─ N-2 ─ N-1 ─ N (finalized) ─ N+1 ─ N+2 ─ N+3 ─ N+4 (leaf)
       │                                           │
       │                                           │
       └─ Relay Parent                             └─ Scheduling Parent
          (Execution Context)                         (Scheduling Context)
          • Messages available                        • Backing group selection
          • Configuration (old)                       • Core assignment
          • Randomness                                • Session validators
          • Can be finalized/old                      • Must be recent leaf
          
Candidate Descriptor v3:
┌──────────────────────────────────────────────────────────────┐
│  relay_parent: Hash = N                                      │
│    └─> Used for: execution environment, validation          │
│                                                              │
│  scheduling_parent: Hash = N+4                               │
│    └─> Used for: backing group, core, approval checkers     │
│                                                              │
│  scheduling_session: SessionIndex                            │
│    └─> Used for: determining validator set for disputes     │
└──────────────────────────────────────────────────────────────┘

Key Insight:
  Parachain Block (permanent) ──uses──> Relay Parent N (can be old)
  Candidate/POV (transient)   ──uses──> Scheduling Parent N+4 (recent)
  
  If candidate fails to get included, the same parachain block can be
  resubmitted in a NEW candidate with a different scheduling parent!
```

We will introduce an additional `scheduling_parent` field in the
`CandidateDescriptor`, in addition to the relay parent we already have, and a
`scheduling_session` field. The first is used to determine core and backers, the
second is used to identify responsible secondary checkers.

The idea is that the scheduling parent is a property of the candidate and the
POV, but not of a regular parachain block. Thus the blocks stay valid,
independent of the validity of the scheduling parent.

### Providing Scheduling Information

The block producer provides scheduling information via a digest item to the
block. An important detail: The relay parent is also an implicit (but arguably
the most important) part of that scheduling information.

This information is then essentially passed through and output via candidate
commitments, where the relay chain can verify it. The passing through the block
is done to ensure that only the block producer can set this information. This
works, but ties the block to the scheduling.

What we want to do in addition now, is to provide a **separate optional entry
point to the PVF**, which we give the following:

- Access to the state of the parachain—at the same block height as the included
  block, or in case of multiple blocks, the block height of the first block in
  the POV
- The scheduling parent
- A header chain with a length defined by the runtime from the scheduling parent
  to an internal scheduling parent
- The relay parent of the candidate
- The scheduling information (which core to use essentially)—signed by the
  responsible collator as of the slot/time information from the internal
  scheduling parent

That function then:

1. Verifies the header chain, in particular that it has the expected fixed
   length
2. Looks up the eligible block producer based on the internal scheduling parent
3. Verifies the signature of the scheduling information
4. Checks that the relay parent is either equal to the internal scheduling
   parent or otherwise not part of the header chain from scheduling parent to
   internal scheduling parent
5. Provides the verified scheduling information for incorporating into candidate
   commitments

If input for the separate entry point is provided, the provided core by the last
block gets overridden. The PVF should still reject any POV that is not "sealed"
with exactly one last block that provides the to be used core via the
commitments. Still providing (and enforcing) the commitment via the last block
allows for two important properties:

1. When producing blocks, nobody but the block producer can wrap up and submit a
   POV—within their slot
2. At the same time, the blocks are then still self contained on the happy path,
   meaning any full node can construct the same POV as the block producer by
   only having received the blocks and can advertise the POV to validators

The header chain and the internal scheduling parent were introduced to enable
the relay parent to be older than the scheduling parent. We can let backers
enforce the scheduling parent to be an active leaf and cheaply replicate the
functionality of building on older relay parents. Additional
benefit/side-effect: Via the scheduling parent we can enforce building on older
relay parents cheaper, as we no longer need to include a proof of the current
relay chain authorities for verification.

### Handling on the Relay Chain

#### Backing Group Selection—Back to Strict Leaf Handling

For candidates making use of the new candidate descriptor, the backing group
will no longer be selected by the relay parent, but by the scheduling parent.

The backers will check the provided scheduling parent in the candidate
descriptor to be equal to one of the tips of the chain: We are back to an
unambiguous backing group selection for a particular candidate.

This is similar to what we had with synchronous backing with regards to the
relay parent. The difference is that, because the scheduling parent is
independent from parachain blocks, collators are still able to produce blocks
ahead of time, just at submission time, they need to check for the most current
leaf and pack the already produced blocks into a candidate accordingly.

Another difference to synchronous backing is that this restriction only applies
to what backers are supposed to accept. Afterwards, the collation stays valid,
according to the system we already have in place for asynchronous backing, with
the only difference that the scheduling parent is now the relevant relay chain
block to determine the lifetime, instead of the relay parent.

#### Sessions and Session Boundaries

In the current implementation we require a candidate to get backed and included
in the session its relay parent appeared in. We do this for at least two
reasons:

1. A session determines the validator set responsible, but we have multiple
   phases: Backing, then inclusion, then approval voting and potentially even
   later a dispute. For simplicity it makes sense to ensure that for all three
   (backing, approvals, disputes) the same validator set is responsible.
2. The number of cores might change on a session boundary. If there are less
   cores than before, then a core might no longer exist.

We invalidate relay parents and thus parachain blocks on session boundaries.
When building on older relay parents the problem gets worse with depth. If we
build at depth 3 for example, the first 3 relay chain blocks will not be able to
back anything as the usable relay parent would still be in the old session—which
is illegal.

These issues get fixed by virtually extending a session as described earlier to
allow blocks to get backed in the last block of a session, and by basing
invariants on the scheduling parent instead of the relay parent.

### New Candidate Descriptor Version

New fields:

- `scheduling_parent`
- `scheduling_session_index`

Existing relay parent is used to provide the correct execution environment to
the candidate validation. The session index information of the relay parent is
used for two things:

1. Checking against the correct host configuration
2. Pruning of old relay parents. We can use `(SessionIndex, BlockHash)` as key
   for lookups then (as the SessionIndex is provided), which allows for
   efficient pruning of old relay parents, based on SessionIndex

**Semantics**: The extended possible age of relay parents will only be allowed
for parachains which upgraded to the new descriptor, as we need to fallback to
the relay parents for scheduling for candidates of version 2 and below.

### Backwards Compatibility & Upgrade Path

Supporting of older relay parents will be limited to candidates that make use of
the new candidate descriptor version supporting the scheduling parent. For
candidates of older versions, the relay chain will continue to operate as it
used to be, except for universal fixes.

Enforcing the new descriptor once used should not be required from the relay
chain perspective. For the PVF, it will accept an optional scheduling parent—if
provided (`CandidateDescriptor` v3)—it will expect separately provided signed
core information, otherwise fall back to the old behavior.

---

## Acknowledgements

### Basic Rules

We build low-latency confirmations on top of an in principle forky block
production. Creating forks is legal and in some situations necessary. For
long-latency, fork selection happens via the relay chain, for low-latency,
acknowledgements do the trick. By acknowledging a block X we as a collator make
the following commitments:

1. **We will build on that block X or a descendant of that block when it is our
   turn**, this should imply:
   - (a) We have seen an acknowledgement for the block X by the block producer
     of the parent parachain block (If that block producer is us, replace this
     rule with the next commitment: (2))
   - (b) We have seen an acknowledgement from the block producer of block X for
     the parent parachain block
   - (c) We have fully imported the block and can confirm its validity

2. **We confirm that the produced block picked the right/latest parent produced
   by us**, if we had been the block producer of the parent parachain block of X

3. **We will not acknowledge any other block with the same parent**

4. **The whole ancestry of the block was either acknowledged too or finalized by
   the relay chain**. A block is acknowledged if at least the block producer and
   the one coming after have acknowledged the block, which implies also (2)
   because of (1a): Therefore "acknowledged" translates to 2 acknowledgments for
   in-slot blocks and 3 acknowledgments for blocks at the slot boundary
   (minimum)

5. **Safeguard**: Don't acknowledge if relay chain finality is lagging
   significantly—in particular, when building on finalized relay parents, don't
   acknowledge if the relay parent is not yet finalized! Thus for chains that
   want to put block confidence to the max, collator based finality (to deserve
   the name), will be dependent on relay chain finality.

6. **Timing**: Don't acknowledge a block with a relay parent that is older than
   necessary, based on the leaves in your view. These blocks are late, which
   means no low-latency anyways and by not acknowledging we are not forced to
   build on them: This prevents a straight-forward censorship attack, where the
   previous collator just builds all their collations late, gets them
   acknowledged but then leaves the duty of submission to the next collator.

Point (2) is arguably the most important one and deserves some clarification:
One reason for forks is block propagation latency. It can happen that a block
producer has not seen some block and therefore might build on one of its
ancestors (the latest block it actually has seen). We can rule that possibility
out, by having the previous block producer acknowledge that our block is indeed
built upon the latest block it produced.

```
Acknowledgement Flow - The Critical Dependencies
═════════════════════════════════════════════════

Scenario 1: SLOT BOUNDARY (different producers)
────────────────────────────────────────────────

Collator A (slot 1):
  Produces Block N
  → ACK_A(N) - acknowledges own block (after verifying rules)

Collator B (slot 2):
  Receives Block N
  Must verify before acknowledging N:
    ✓ Rule 1a: See ACK from parent producer - N/A (see bootstrapping section below)
    ✓ Rule 1b: See ACK from N's producer for parent - N/A  
    ✓ Rule 1c: Fully imported and valid
  → ACK_B(N)
  
  ✓ Block N is now "acknowledged" (ACK_A + ACK_B)
  
  Produces Block N+1 (parent: N)
  
  CANNOT yet ACK N+1! Must wait for rule 1a...

Collator A:
  Receives Block N+1
  
  ⚠️  CRITICAL CHECK (Rule 2):
      "Does N+1 build on MY LATEST block?"
      
      If A produced another block after N → CANNOT acknowledge N+1
      If N is still A's latest → Can acknowledge N+1
      
  → ACK_A(N+1) - CERTIFIES: no fork from A, N+1 builds on A's latest
  
  ⚠️  This is NOT automatically true! A must verify no fork exists.

Collator B:
  Receives ACK_A(N+1)
  NOW can verify rule 1a for N+1:
    ✓ Rule 1a: ACK_A(N+1) received (parent producer acknowledged child)
    ✓ Rule 1b: ACK_B(N) already sent (child producer acknowledged parent)
    ✓ Rule 1c: N+1 is valid (B produced it)
  → ACK_B(N+1)

Collator C (slot 3):
  Receives Block N+1 and acknowledgements
  Before acknowledging N+1, must verify:
    ✓ Rule 1a: ACK_A(N+1) exists ← parent producer certified no fork!
    ✓ Rule 1b: ACK_B(N) exists ← child producer saw parent
    ✓ Rule 1c: Block valid
  → ACK_C(N+1)
  
  ✓ Block N+1 is now "acknowledged" (ACK_B + ACK_A + ACK_C = 3 minimum)


Scenario 2: IN-SLOT (same producer, multiple blocks)
─────────────────────────────────────────────────────

Collator A (produces N and N+1 in same slot):
  Produces Block N
  → ACK_A(N)
  
  Produces Block N+1 (parent: N)
  
  Rule 1a for N+1: "parent producer = me" → use Rule 2 instead
  Rule 2: "N+1 builds on my latest" → YES (self-evident)
  → ACK_A(N+1)

Collator B:
  → ACK_B(N)
  ✓ Block N is "acknowledged" (ACK_A + ACK_B = 2)
  
  → ACK_B(N+1)  
  ✓ Block N+1 is "acknowledged" (ACK_A + ACK_B = 2)


Key Security Property:
──────────────────────
At slot boundaries, parent producer A CANNOT acknowledge child block N+1
until A verifies N+1 builds on A's latest. If A produced a fork after N,
A cannot send ACK_A(N+1) without being provably slashable (rule 3).

Once ACK_A(N+1) is sent, A is COMMITTED - any fork from A is now provable.
This is why acknowledgements are separate from block production.
```

With the previous block producer having acknowledged the child block, it is safe
for all other collators to acknowledge the block too, giving guarantees of (1)
and (3), because the block producer is no longer able to interfere anymore:

1. It is illegal to produce a block with the same parent in the same slot
2. The collator is not allowed to produce a block based on some older parent
   either, as it already acknowledged the parent block, see (1b) above
3. If the block producer of X fails to submit the block to the relay chain, the
   next block producer will be able to pick it up and submit it, with a new
   scheduling parent

### Recovery & Bootstrapping

In case of a malfunctioning collator (not providing approvals as it should) the
system can recover once the next block producer took over and has confirmation
from the relay chain that it is building on the right block. This is safe to do
once we see at least one block of ours (the first one) to get backed on chain.
The same situation applies to first "powering up" the system. It might take a
while until we see our block on the relay chain, but once we see it
acknowledgments will start getting sent out and once caught up, low-latency
confirmations are in place.

### Ensure Block Submission

Each collator, when acknowledging a block, commits to that block becoming
canonical. Therefore when it is its turn, it either finds the block already on
or on its way to the relay chain or will itself make sure it ends up there.

In particular when it is a collator's turn to produce a block, it will first
check whether all blocks it acknowledged have made their way to the relay chain.
It will:

- Check for inclusion on finalized blocks—any such blocks no longer need to be
  monitored at all
- Check for inclusion on active leaves
- Check for backed on active leaves
- If the relay parent of the candidate is too young to have made it on chain
  yet, it will ask the currently assigned backing validators for confirmation

If all of these checks fail, then the collator will use the assigned core to
resubmit the missing collation by that other block author, instead of sending
its own.

We query the currently assigned backing group as it is rotating; if we stuck
with the originally responsible backing group, we would amplify censorship
abilities: In this context telling us that a candidate was received, when it was
not (or the other way round), causing us to build/send a collation that won't
make it/is redundant. The currently assigned backing group already has the
ability to censor, so there is no additional threat.

Apart from using their own slot to provide missing collations, a collator will
also always advertise any collation acknowledged in addition to the original
block producer during their slot already. This behavior is mandatory, as
otherwise our acknowledgments can be abused for censorship: The previous block
author could not submit any collation on purpose, leaving the whole work to
us—in our slot, resulting in perfect censorship.

### Rewards

Rewards for timely acknowledgments: Given that the relay chain forces us
reliably to advance time, we can actually have some sanity checks on the
timeliness of acknowledgments \- by putting received acknowledgments in the next
block for rewards. This is far from perfect, but might still make sense:

1. The only reason, apart from pure griefing to delay acknowledgments is to get
   more assurance, that you won’t get slashed. This should really not be
   necessary to begin with, but more certainty will hardly come before even the
   next block has been produced. Thus delaying this little bit seems hardly
   worth the trouble. 
2. At least on slot boundaries, you don’t necessarily know exactly when exactly
   the next author will produce a block (especially with dynamic block times),
   thus by delaying your acknowledgment, you will risk not getting it in. Also
   the block producer might legitimately decide to not put your acknowledgement
   in, if it came late. 
3. We can enforce transmission of the acknowledgement to at least the next
   collator, by only granting rewards for your acknowledgments, if all necessary
   acknowledgments are provided. 4. With many collators and all of them
   acknowledging, we will not want to put all acknowledgements in blocks,
   instead honest collators would put in just a few, based on a first come \-
   first serve basis, thus incentivizing collators to be fast with their
   acknowledgments.

Limitations: If we enforce having received acknowledgements for a block, by the
time we build the next block to get rewards, we limit how fast we can go with
blocks. Obvious mitigations:

1. Still accept acknowledgments in some later block (with some limit). 
2. Don’t do rewards for acknowledgements. There is little incentive to alter the
   client and this kind of protocol violation of not doing acknowledgements and
   it is relatively harmless (only service degradation): Misbehaving nodes can
   be dealt with by governance. 

# Punishments/Enforcing the rules for a canonical chain
{#punishments/enforcing-the-rules-for-a-canonical-chain}

With acknowledgements and decoupling in place, we can hold collators accountable.

## Offenses {#offenses}

| Offense # | Description | Violated Rule | Proof Required | Severity |
|-----------|-------------|---------------|----------------|----------|
| **1** | Direct equivocation: Produce two blocks in the same slot with the same parent | Illegal to produce multiple blocks with same parent in same slot | Two blocks with same slot, same parent, different hashes, both signed by collator | High - Clear malicious intent |
| **2** | Acknowledge two blocks with the same parent | Rule 3: Will not acknowledge any other block with the same parent | Two acknowledgement signatures for different blocks with same parent | High - Breaking canonical chain commitment |
| **3** | Acknowledge a child block you built, then build a conflicting child | Rules 1b + 3: Cannot build conflicting block after acknowledging parent | ACK for parent block + conflicting child block produced afterward | High - Fork after commitment |
| **4** | Acknowledge a block from previous slot, then build conflicting block with same parent | Ensure Block Submission: Must resubmit acknowledged blocks, not replace them | ACK for block from previous slot + conflicting block with same parent | High - Censorship/replacement attack |

## Punishment {#punishment}

Especially if we deployed safe-guards against accidentally running multiple
instances of the same collator (which would lead to equivocations), we should be
able to slash drastically on misbehavior (e.g. 100%) as we ruled out any kind of
honest misbehavior (in the absence of bugs of course).

Recommendation would be to roll out with minor punishments, e.g. only losing out
on rewards for the era and monitor exactly any punishments from occurring. Once
we gain enough confidence that misbehavior never occurs in practice, we can ramp
up. 
   
Given the above list of offenses, it becomes clear that there is always exactly
one collator misbehaving that can be punished, thus the economic security is
limited to the stake of a single collator, regardless of how many
acknowledgement signatures have been received. 

More acknowledgements are still useful as they increase confidence for bad
network conditions. This is because each collator acknowledging a block, made a
commitment to get it on the relay chain, if it did not end up there yet, see
offense (4).

## Implementation {#implementation}

All the above offenses should be easily provable to the parachain runtime. We
will define a challenge window (number of blocks) within which a proof of an
offense must be submitted. For that window the runtime should keep around any
information necessary for proofs to be verified. 

The duration of the challenge window should be as large as possible to make
censorship attempts costly, but should be at least long enough that all
collators had at least one slot/opportunity to submit a proof. This is really
the absolute lower end, preferably we allow much larger windows at the cost of
for example more expensive/heavier proofs.

## Disaster Recovery {#disaster-recovery}

We are greatly extending the time a relay parent stays valid, but there will
still be a limit. Once the relay parent of a parachain block goes out of scope,
a new conflicting block can be built (with a current relay parent), without the
risk of any punishment. I don’t actually see any realistic reason why this
recovery would ever be necessary (all acknowledging collators would need to
somehow have forgotten about the block or we have a bug and the relay chain does
not accept the candidate for some reason), but it is worth mentioning that it
exists. 

# Speculative Messaging {#speculative-messaging-1}

To become immune to relay chain forks we want to build on an old and already
finalized relay parent. This will impact messaging latency. To mitigate this
tradeoff, an implementation of this design should come with an implementation of
speculative messaging, which allows for fast, almost instant messaging
independent of the age of the relay parent.

This will be achieved by communicating messages off-chain and instead of
verifying proofs based on the relay chain context, we will have additional
candidate commitments. For receiving messages, a parachain will emit a
“*requires*” commitment, for sending a parachain will emit a “*provides*”
commitment. The relay chain will ensure that a parachain block will only get
included if its requirements are matched by a providing parachain.

With this in place, we can bring down messaging latency to instantaneous. E.g.
for low-latency chains, we would extend the acknowledgement rules, such that you
are only allowed to acknowledge a block if any received messages are provided by
blocks, which are either already seen on the relay chain (slow) or are coming
from another low-latency parachain and you have seen the sending block
acknowledged (fast).

Based on that example it becomes obvious how speculative messaging and
low-latency chains provide a very unique and exceptionally powerful combination,
that I am not aware of anyone else is building:

Trustless and decentralized low latency confirmations combined with basically
instant trustless messaging latency to other chains. 

# Implementation Details {#implementation-details}

## Collator Communication {#collator-communication}

### Req/response protocol to query collation status {#req/response-protocol-to-query-collation-status}

For ensuring block submission, collators need to have a low-latency way of
finding out whether a particular block/candidate is known by the validators
already. We will introduce a request/response protocol where collators can ask
validators for candidates in prospective parachains. Having a request for all
para header hashes currently live/in-flight on validators for a particular
parachain should do the trick. 

Requests on this protocol should take collator reputation into account. If that
protocol gets spammed, we should prefer requests from peers with good/existing
collator reputation.

### Next collator advertisements {#next-collator-advertisements}

To maximise chances, e.g. in the event of connectivity issues, attacks and
*censorship attempts* for a collation to make it to the backers, all
acknowledging collators, will also prepare the exact same collation as the
original author (building the POV and the candidate) and advertise it to
backers. All collators who acknowledged should advertise the collation as they
are otherwise susceptible to censorship attacks. 

## Relay Chain {#relay-chain-1}

### Relay Chain Runtime {#relay-chain-runtime}

We need to allow much older relay parents and we do have certain checks in the
relay chain runtime about the relay parent of a candidate:

- Basic: Is the relay parent even part of this chain? 
- What is the block number \- used for determining the water mark, checking the
  advancement rule, … 
- Storage root \- for verifying the persisted validation data. 

Solution: Just store data for more relay parents. E.g. similarly to how we store
*all* included candidates for the last 6 sessions
[here](https://github.com/paritytech/polkadot-sdk/blob/5314442a060f53391c5d8d1ece4937332dfa9fc9/polkadot/runtime/parachains/src/disputes.rs#L424).

In particular we can also store by SessionIndex (SessionIndex, relay parent
hash). This way we can equally easily prune old parents, based on the session.
Lookup is easy, as the candidate receipt contains the SessionIndex of the relay
parent since version 2\. For old candidates, we won’t support
high-confidence/low-latency.

Allowing something like the last 14\_400 relay chain blocks as relay parents
should suffice for “perfect” confidence:

1. We rotate backing groups every 10 blocks, thus you get a chance to try every
   backing group several times before the relay parent goes out of scope. With ⅔
   honest, censoring is not possible. 
2. For that limit to ever cause problems in practice, we would need to be in a
   situation where the parachain can not submit a collation in 14\_400 relay
   chain blocks, while the relay chain is making progress. Thus the relay chain
   is actually fully operational, if it still produces blocks every 6s. With
   [forced
   backoff](https://github.com/paritytech/polkadot-sdk/issues/633#issuecomment-2422484619)
   implemented, this would even extend to finality still being fine. 3.
   Realistic reasons I can think of, why the relay chain should be fully
   operational, yet the parachain is not able to submit a collation are issues
   on the parachain side, specifically on the node/networking. This is where the
   14\_400 blocks suggestion is coming from \- this is a full day worth of
   blocks. The assumption would be that such issues can be resolved within 24
   hours. In particular, the situation would be that we have a fully operational
   relay chain and actually an operational parachain (it produces blocks and is
   able to gather acknowledgements), yet it is not able to submit collations.
   For this to cause any harm, those blocks and acknowledgements would also need
   to make it to users, yet we are still not able to submit blocks to the relay
   chain. This scenario seems so unlikely that it can realistically only be
   caused by either a bug or an attack. For a bug, we should indeed be able to
   rollout a fix within 24 hours (node side), for an attack: We need to invest
   in network hardening and defense \- a blockchain that can easily be 100%
   DOSed for 24 hours, is hardly “unstoppable”. 

# Threats {#threats}

Threats considered in this design and to what extent, also hinting further
improvements outside of the scope of this document.

## Block withholding \- censor the next guy
{#block-withholding---censor-the-next-guy}

No acknowledgements by design. Fetching directly from backers can help, but is
susceptible to DOS. E.g. even with the collator protocol revamp \- all collators
will have a good reputation and could all try to fetch from the backers,
delaying delivery to the next block author (enough). 

Mitigations:

- Pre-PVF: Backers can prefer delivering POVs to the next eligible block author
  \- proven by a Pre-PVF. The Pre-PVF would be the same entry point as we use in
  the POV for providing scheduling information. 
- Determine submission/collation rights only by most current active leaf: This
  allows forcing block submissions on a timely basis, otherwise one loses the
  submission right and it is the next block producers turn to submit collations.
  Strict leaf handling also makes Pre-PVF checks more effective, as then really
  only legitimate requests will be prioritized. 

## Submit different block {#submit-different-block}

Providing one block to the network, but submit a different one to the relay
chain. This threat should be mitigated: Acknowledgment signatures will in that
case either not be present (no threat) or some collator will get slashed.

## Omit block submission {#omit-block-submission}

Resolved in this design, because the follow up collator will check with backers
and submit the collation if it wasn’t submitted yet. By having other collators
also advertise the collation, this is also much harder to achieve.

## Equivocations & Nothing at stake problem
{#equivocations-&-nothing-at-stake-problem}

We are having multiple blocks per slot, so the classic: multiple blocks per slot
\== equivocation does not work. Luckily this and the “Nothing at stake” problem
are a non-issue for (low-latency) parachains:

- Low-latency is based on acknowledgements. Here we are enforcing to only sign
  off one canonical chain. Acknowledging two blocks with the same parent is
  illegal and will be punished. 
- For non low-latency: We have fork selection via the relay chain, therefore it
  does not matter if collators build on multiple forks as we are guaranteed
  still that only one of them will survive.

One simple rule we still need to enforce though with regards to forking: It is
illegal to produce two blocks with the same parent in the same slot.

# Research {#research}

## Limitations on block times {#limitations-on-block-times}

### Slot to Ping Ratio {#slot-to-ping-ratio}

#### Literature {#literature}

On Ethereum people are concerned about too fast blocks, because of the “slot to
ping ratio”. Do these concerns apply to Polkadot?

This article sheds on how slots work on Ethereum:
[https://www.paradigm.xyz/2023/04/mev-boost-ethereum-consensus](https://www.paradigm.xyz/2023/04/mev-boost-ethereum-consensus)

Fun fact from this article:
[https://figment.io/insights/beyond-the-basics-understanding-rewards-on-ethereum/](https://figment.io/insights/beyond-the-basics-understanding-rewards-on-ethereum/)

An Ethereum validator only proposes a block once every 5 months\!

Takeaways from
[https://eth2book.info/capella/part2/consensus/lmd\_ghost/](https://eth2book.info/capella/part2/consensus/lmd_ghost/)
:

- Attestations are introduced to make the network more stable with regards to
  reorgs, than by simply using the longest chain rule, with regards to network
  latency. 

Shedding a bit more light on the problem:

[https://ethresear.ch/t/timing-games-implications-and-possible-mitigations/17612/4](https://ethresear.ch/t/timing-games-implications-and-possible-mitigations/17612/4)

#### Conclusion {#conclusion-1}

The situation on Ethereum is quite different, as they have a different form of
consensus. Instead of simply letting the longest chain rule decide for a fork,
they have “attestations” which are approving messages sent by other validators
still within a 12s slot. The idea is that the next block producer is supposed to
build on the block with the most votes (build on the block, more validators
agree on). This is supposed to make the network more stable with regards to
reorgs and forks, but is having its own problems in practice. For a start, it
obviously has a performance impact, because the next block producer not only
needs to receive the previous block in time, but also attestations from other
validators. Second it adds a lot of complexity and probably has very little
benefit due to validators playing timing games.

What do we get out of these for Polkadot and low-latency parachains? First our
acknowledgements are similar in nature to Ethereum attestations and we might run
into similar problems long-term. In particular it might become profitable to
delay a block, so long that it becomes tough for the next block producer to
still build on it, without losing out on block production opportunities itself. 

The actual problem with the slot to ping ratio seems to come from the assumption
that one produces one block per slot. Then of course too short slot times can
easily lead to a block producer not receiving the previous block in time to not
miss its own slot.

All of these problems are greatly mitigated by breaking the one slot \- one
block assumption and making collators produce multiple blocks per slot. What
seems to be problematic is short slot times, not so much short block times. 

The question now becomes, why is Ethereum not considering multiple blocks per
slot?

### Multiple blocks per slot {#multiple-blocks-per-slot}

From the previous section, it seems that not actually short block times are
problematic, but rather short slot times. This can easily be resolved by
authoring multiple blocks per slot, but what are the downsides here?

1. Proving and punishing equivocations becomes more complex: The simple rule,
   equivocation \== multiple blocks per slot obviously no longer applies. 
2. A single block producer stays in control of the chain for longer.

### Constant per-block overhead {#constant-per-block-overhead}

- Header 
- Proof \- not an issue with Basti blocks, as proof will be per POV not per
  block. 
- on-initialize/on-finalize

According to Basti constant overhead is low. We should be well below 10ms with
regards to execution time.

## Shreds/Preconfirmations in Ethereum {#shreds/preconfirmations-in-ethereum}

[https://blog.risechain.com/incremental-block-construction/](https://blog.risechain.com/incremental-block-construction/)

Takeaways:

- Batch merkelization \- work with changesets in between for better efficiency. 
- They seem very much to rely on a centralized sequencer. E.g. the possibility
  that the current sequencer was not able to submit the block the the L1, is not
  even considered. Which is fine, if there is only one sequencer \- as it will
  succeed eventually, but problematic if there were multiple and they take turns
  as then you would need collaboration to maintain guarantees. Even if we
  ignored that fact, any taking turns would need to be orchestrated and likely
  needs to be verified by the L1, which brings complexity: You really don’t want
  forks, if you want low-latency confirmations. TL;DR: They get away with a lot
  less complexity, because of centralization.

## We are all building the same thing {#we-are-all-building-the-same-thing}

[https://dba.xyz/were-all-building-the-same-thing/](https://dba.xyz/were-all-building-the-same-thing/)

Takeaways:

1. Ethereum people are trying to build something very similar to what I had in
   mind with speculative messaging. Instead of using the base layer, they use
   Agglayer for cross-chain communication. 2. Preconfs have a big problem: MEV\!
   Giving a confirmation early risks the block author losing out on an MEV
   opportunity. Thus it is hard to incentivize this properly. Easier: On a block
   base \- if we have fast blocks, by the time the block is built there is no
   further MEV capability. There actually exists a related problem with block
   authors authoring multiple blocks in a slot: It might be beneficial to build
   less blocks (delay) to maximise MEV. 

## Raptor Cast & Turbine {#raptor-cast-&-turbine}

[https://docs.monad.xyz/monad-arch/consensus/raptorcast](https://docs.monad.xyz/monad-arch/consensus/raptorcast)  
[https://solana.com/news/turbine---solana-s-block-propagation-protocol-solves-the-scalability-trilemma](https://solana.com/news/turbine---solana-s-block-propagation-protocol-solves-the-scalability-trilemma)

Takeaways:

For elastic scaling so far we had mostly been focused on making this work from a
block producer and relay chain perspective, but if we push limits \- go fully
pipelined, our classic produce \- propagate \- import cycle no longer works for
full nodes. The above techniques have been designed to allow for a large
validator set though, so only directly become relevant if we want to have very
larger collator networks.

## Ethereum Preconfirmations {#ethereum-preconfirmations}

[https://www.luganodes.com/blog/preconfirmations-explained/](https://www.luganodes.com/blog/preconfirmations-explained/)

[https://stakely.io/blog/guide-preconfirmations-ethereum-what-they-are-how-they-work-why-they-matter](https://stakely.io/blog/guide-preconfirmations-ethereum-what-they-are-how-they-work-why-they-matter)

Takeaways:

Preconfirmations are coming from L1 block proposers \- they commit to submitting
an L2 bundle later. The first article is quite hand-wavy, lot’s of unanswered
questions. Definitely a very complex system with hard to enforce guarantees. 

The second article makes it immediately more clear how it works and what
problems it solves. E.g. getting a confirmation from the L1 validator should
help with censorship resistance. The concept exploits that everything is
Ethereum and everyone \- including validators understand Ethereum transactions.
An L1/0 transaction level confirmation on Polkadot is definitely not that
straight forward as parachains are heterogeneous and their transactions unknown
to the validators. 

Is there a lot to be gained by getting the confirmation from the L1/0? I think
in Ethereum’s case it makes sense for the following reasons:

1. It could indeed help with censorship resistance, especially given centralized
   rollup sequencers. 
2. They need a way to extract value out of the L2s into the L1.

Major downsides:

1. Complexity: You put layer upon layer upon layer, but don’t have them
   separated at all. Instead they highly interact with each other and every
   layer has very deep assumptions about how the other layers work. It is a
   spaghetti architecture: E.g. L1 validators need to care and understand
   individual transactions in an L2 block. 2. Getting a confirmation from an
   individual L1 validator implies that slashes can not be too high \- as things
   can easily go legitimately wrong. 3. Even more complexity (see (1)): My
   understanding is that the validator commits to a preconfirmation even before
   the L2 block is built \- or seen by the validator. This implies that if the
   sequencer fails for some reason, the validator itself needs to build the L2
   block to uphold its promise \- or it can’t, which would again limit how much
   you can punish, limiting the usefulness to have confirmations coming from
   highly staked L1 validators. 4. I don’t buy the MEV advantage: If the
   validator is aware of an MEV opportunity it would miss by signing the preconf
   \- it would just not give any.

Doing the same on Polkadot is not easily possible, because of the heterogenous
nature and because we have more layers. E.g. a promise from the backer, is worth
less because it could still fail because of the block producer \- and the other
way around.
