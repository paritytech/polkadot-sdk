# Parachain Service on JAM

> **Status**: Draft  
> **Authors**: TBD  
> **Last Updated**: 2026-03-02

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [The Parachain Service](#3-the-parachain-service)
   - 3.1 [Service State Layout](#31-service-state-layout)
   - 3.2 [Work Items](#32-work-items)
   - 3.3 [Work Reports](#33-work-reports)
4. [Refine: In-Core Execution](#4-refine-in-core-execution)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
6. [Authorization & Coretime](#6-authorization--coretime)
7. [Collator Protocol Changes](#7-collator-protocol-changes)
8. [Messaging & XCM](#8-messaging--xcm)
9. [Migration Path](#9-migration-path)
10. [Open Questions](#10-open-questions)
11. [References](#11-references)

---

## 1. Overview

This document describes the architecture of the **Parachain Service** — a JAM service that implements
Polkadot's parachain host functionality. The Parachain Service is the JAM successor to the current
Polkadot relay-chain parachain host, mapping all the concepts of collation, validation, availability,
and finality into JAM's Collect-Refine-Join-Accumulate (CRJA) computation model.

In short: what the Polkadot relay chain currently does natively for parachains will, on JAM, be
implemented as an ordinary JAM service. This enables the relay chain to shed protocol-level
opinion about parachains, while parachains continue operating with full shared security and
interoperability.

The key conceptual mapping from today's Polkadot to JAM:

| Polkadot 1.x | JAM | Role |
|---|---|---|
| Collation (collator builds candidate + PoV) | **Collect** | Gather inputs off-chain |
| Backing (validator group checks PoV) | **Refine** | Stateless off-chain validation |
| Availability + Approval | **Guarantees + Assurances** | Attest correctness, ensure data available |
| Inclusion + on-chain parachain consensus | **Accumulate** | Integrate results into shared state |

### Scope

This document covers:

- How a parachain block's lifecycle maps onto JAM's Work Package / Refine / Accumulate model
- The service state layout and key data structures
- How authorization and coretime allocation integrate
- Changes to the collator role
- Cross-chain messaging under the new model
- A migration path from Polkadot 1.x parachains

This document does **not** cover JAM fundamentals in depth; readers are assumed to be familiar with
the [JAM Gray Paper](https://graypaper.com) concepts (services, work packages, refine, accumulate,
guarantors, etc.).

---

## 2. Architecture Overview

The following diagram illustrates where the Parachain Service sits in the overall JAM
architecture. A JAM service provides **both** in-core code (Refine, executed off-chain by
guarantors) and on-chain code (Accumulate, executed by all validators). The Parachain
Service spans both domains:

```
 ┌──────────────────────────────────────────────────────────────────────┐
 │                         JAM Chain                                    │
 │                                                                      │
 │  On-Chain    ┌────────────────────────────────────────────────────┐  │
 │              │              Services Layer                        │  │
 │              │  ┌──────────────────────┐   ┌──────────────────┐  │  │
 │              │  │  Parachain Service   │   │  Other Services  │  │  │
 │              │  │  ┌────────────────┐ │   │                  │  │  │
 │              │  │  │  accumulate()  │ │   │  accumulate()    │  │  │
 │              │  │  └────────────────┘ │   │                  │  │  │
 │              │  └──────────┬───────────┘   └──────────────────┘  │  │
 │              └─────────────┼────────────────────────────────────┘  │
 │  ·····················│····│····································   │
 │  In-Core              │    │                                      │
 │              ┌─────────────┴───────────┐                          │
 │              │  Parachain Service       │                          │
 │              │  ┌────────────────┐     │                          │
 │              │  │   refine()     │     │                          │
 │              │  └────────────────┘     │                          │
 │              └─────────────────────────┘                          │
 │              ▲                                                     │
 │              │ Guarantors execute Refine on assigned cores         │
 │                                                                      │
 │  ┌────────────────────────────────────────────────────────────────┐  │
 │  │    Data Availability (erasure-coded)                           │  │
 │  └────────────────────────────────────────────────────────────────┘  │
 └──────────────────────────────────────────────────────────────────────┘
```

The CRJA pipeline for a parachain block:

```
[Collect]     Collator gathers transactions, builds a parachain block candidate + PoV
    │
    ▼
[Refine]      IN-CORE: Guarantors execute the PVF against the PoV.
              Stateless, off-chain, metered via PVM gas.
              Output: a compact Work Result (~90 kB) summarising the validated candidate.
    │
    ▼
[Join]        The Work Report (aggregating Work Results) is submitted on-chain.
              JAM validators attest (guarantee) its correctness. Availability is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate function runs on-chain.
              It records the new parachain head, processes queued messages, handles
              code upgrades, and updates coretime accounting.
```

---

## 3. The Parachain Service

The Parachain Service is a JAM service whose code implements the on-chain logic of the parachain
host. It holds all per-parachain state and drives the CRJA pipeline for every registered parachain.

### 3.1 Service State Layout

The service state is a key-value store. Logically it contains:

```rust
// Top-level service state (conceptual, not final)
struct ParachainServiceState {
    /// All registered parachains and their current metadata.
    parachains: Map<ParaId, ParaInfo>,

    /// Pending candidates awaiting availability + accumulation.
    pending_availability: Map<ParaId, PendingCandidate>,

    /// HRMP channel state (open channels, queued messages).
    hrmp_channels: Map<HrmpChannelId, HrmpChannel>,

    /// Downward message queues (from Parachain Service → parachain).
    dmq: Map<ParaId, VecDeque<DownwardMessage>>,

    /// Coretime assignments: which core is allocated to which para.
    core_assignments: Map<CoreIndex, CoretimeAssignment>,

    /// PVF (Parachain Validation Function) preimage registry.
    /// Maps validation_code_hash → (code, ref_count, expiry).
    pvf_registry: Map<ValidationCodeHash, PvfEntry>,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Hash of the currently active validation code.
    validation_code_hash: ValidationCodeHash,
    /// Scheduled code upgrade, if any.
    next_validation_code: Option<(ValidationCodeHash, UpgradeAt)>,
    /// HRMP watermark (last relay-parent at which HRMP was processed).
    hrmp_watermark: BlockNumber,
}
```

The **lookup-anchor** mechanism (JAM's preimage store) is used to make PVF code available to the
Refine step, since Refine is stateless and cannot directly access service storage. Guarantors
performing Refine can fetch the PVF bytecode via its hash, provided it has been registered.

### 3.2 Work Items

Each work package submitted to the Parachain Service contains one or more **work items**. For the
Parachain Service, a work item corresponds to one parachain candidate:

```rust
/// A work item for the Parachain Service encodes a single parachain candidate.
struct ParachainWorkItem {
    /// The parachain this candidate belongs to.
    para_id: ParaId,

    /// Compact candidate descriptor: relay parent, collator signature, commitments hash.
    descriptor: CandidateDescriptor,

    /// The Proof-of-Validity (PoV) — the actual block data + witness.
    /// This is the large input to Refine (up to ~15 MB per slot across all items).
    pov: PoV,

    /// Persisted validation data needed to execute the PVF.
    persisted_validation_data: PersistedValidationData,
}
```

Multiple `ParachainWorkItem`s (i.e., candidates for different parachains) can be batched into a
single work package, up to the per-slot data limits imposed by JAM.

### 3.3 Work Reports

After the Refine step, guarantors produce a **work result** per work item. The work report
aggregates these results and is what gets submitted on-chain:

```rust
/// Output of the Refine step for one parachain candidate.
struct ParachainWorkResult {
    /// Which parachain this result belongs to.
    para_id: ParaId,
    /// The output of executing the PVF: head data, upward messages, new code, etc.
    candidate_commitments: CandidateCommitments,
    /// Hash of the candidate receipt, for cross-referencing.
    candidate_hash: CandidateHash,
    /// Whether validation succeeded.
    outcome: ValidationOutcome,
}

enum ValidationOutcome {
    Valid,
    Invalid { reason: InvalidReason },
}
```

The work report itself is subject to JAM's **guarantee** and **assurance** mechanisms — validators
attest its correctness and data availability is enforced before Accumulate runs.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

The Refine entry point of the Parachain Service executes the **Parachain Validation Function
(PVF)** for each work item. This is an off-chain, stateless computation performed by guarantors
(validators assigned to the relevant JAM core).

Refine:
1. Fetches the PVF bytecode via the lookup-anchor (using `validation_code_hash`).
2. Instantiates a child PVM with the PVF.
3. Executes the PVF against `(persisted_validation_data, pov)`.
4. Returns a `ParachainWorkResult` with the committed outputs.

Because Refine is stateless, it cannot write to service storage. The only "statefulness" it can
exercise is via preimage lookups — which is exactly how PVF code is accessed.

### 4.2 PVF Execution in PVM

In Polkadot 1.x, PVFs are compiled to WebAssembly. On JAM, PVFs will target the
**Polkadot Virtual Machine (PVM)**, which is based on RISC-V. Key implications:

- PVF code must be recompiled (or retargeted) for RISC-V. Since RISC-V is an official LLVM target,
  this can largely be done via the same LLVM toolchain used for Wasm today.
- **Metering**: PVM provides instruction-level gas metering "for free" (unlike Wasm which requires
  instrumentation). This removes the need for the separate benchmarking-based weight system used
  by the current PVF executor.
- **Execution time**: up to 6 seconds of PVM gas per Refine invocation (one full JAM slot), compared
  to the current ~2-second PVF timeout. This gives parachain runtimes more headroom.
- **Memory model**: Substrate currently assumes a maximum of 4 GiB addressable memory. While
  PVM is 64-bit (RISC-V RV64), the available memory for PVFs remains restricted to 4 GB.
  This means parachain runtimes can continue using 32-bit pointers and addressing — no
  changes to memory layout or pointer-width assumptions are required for the migration.

### 4.3 Continuations

PVM's RISC-V-based design means the execution stack lives in memory, enabling **continuations**:
a computation can be paused and resumed. The Parachain Service can use this to implement
checkpointing for long-running validations, or to spread a single PoV validation across more gas
if needed.

### 4.4 Guarantor Assignment

Guarantors are assigned to cores by JAM's core assignment mechanism. For the Parachain Service:

- The Parachain Service requests a number of cores matching registered parachain capacity
  (via the coretime assignment mechanism — see §6).
- JAM assigns a small validator sub-group to each core for each slot, exactly as today's
  backing groups are assigned to parachains.
- The sub-group executes Refine, produces guarantees, and circulates them for assurances.

This mirrors the current "backing group" model but is now driven by JAM's generic core
assignment rather than a parachain-specific scheduler.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage, roughly ~10ms of PVM gas per work result.

Accumulate for the Parachain Service performs:

1. **Head data update**: Writes the new `head_data` into `ParaInfo` for the parachain.
2. **Message processing**:
   - Upward messages (UMP) from the parachain to the Parachain Service are processed or queued.
   - Downward messages (DMP) already queued are marked as consumed.
   - HRMP messages between parachains are enqueued in the destination para's queue.
3. **Code upgrades**: If `new_validation_code` is present in the commitments, schedules the
   upgrade (subject to a cooldown).
4. **Coretime accounting**: Records that the relevant core was used for this slot.
5. **Dispute handling** (if a conflicting result was judged invalid): initiates the rollback of
   the relevant candidate's effects.

### 5.2 Gas Budget

The Accumulate gas budget per work result is tight (~10ms). Complex message processing or large
HRMP queues must be handled carefully. Options:

- **Batched processing**: Process only a bounded number of messages per Accumulate call; leave
  remainder for the next slot.
- **Deferred actions**: Schedule expensive operations (e.g. code upgrade application) for a
  future Accumulate call.
- **Inter-service messaging**: Delegate some processing (e.g. fee accounting) to other services
  via JAM's `OnTransfer` mechanism.

### 5.3 Disputes and Rollback

JAM's judgment mechanism allows validators to contest work results within ~1 hour. If a
parachain candidate is judged invalid:

- JAM temporarily halts finality for affected blocks.
- The Parachain Service's Accumulate call for the invalid result is rolled back.
- The offending guarantors are slashed.
- The parachain continues from the last valid head.

This replaces the current dispute protocol in the relay chain runtime, leveraging JAM's native
judgment infrastructure.

---

## 6. Authorization & Coretime

### 6.1 Authorization Model

In JAM, any work package targeting a service must be **authorized**. For the Parachain Service,
authorization ensures that:

- Only work packages corresponding to legitimately registered parachains are processed.
- The para submitting the work package holds valid coretime for the requested core.

The authorization check is performed by the Parachain Service's authorization code, evaluated
before guarantors execute Refine. It validates:

```
is_authorized(work_package) =
    para_id ∈ registered_parachains
    AND core_assignment[work_package.core] == para_id
    AND coretime_valid_for(para_id, slot)
```

### 6.2 Coretime Allocation

Parachains obtain coretime from the **coretime chain** (currently `pallet-broker` on Coretime
system chain). The coretime allocation pipeline remains largely unchanged, but the destination
of the assignment changes from "relay chain availability cores" to "JAM service cores".

```
Coretime Chain (pallet-broker)
    │  assigns coretime to para_id on core N
    ▼
Parachain Service (via inter-service message or on-chain assignment)
    │  records core_assignments[N] = para_id
    ▼
Guarantors
    │  execute Refine for para_id's work items on core N
    ▼
JAM chain
```

The exact mechanism for communicating coretime assignments from the Coretime Chain into the
Parachain Service is an open question (see §10).

### 6.3 On-Demand Parachains

On-demand parachains (currently acquired via `pallet-on-demand`) continue to work: they
acquire a single-shot coretime allocation for one slot, submitted as a work package to the
Parachain Service for that slot only. This can be handled via JAM's own bulk/on-demand coretime
markets or via the Parachain Service's own internal priority queue.

---

## 7. Collator Protocol Changes

### 7.1 What Changes for Collators

Collators are responsible for producing parachain block candidates and PoVs, exactly as today.
The primary change is in **how they submit** the candidate:

| Aspect | Polkadot 1.x | JAM Parachain Service |
|--------|-------------|----------------------|
| Submit to | Relay chain backing validators | JAM guarantors assigned to the relevant core |
| Format | `CandidateReceipt` + `PoV` | `ParachainWorkItem` (wraps the same data) |
| Networking | Collator-Validator protocol (req/resp) | Same, targeting JAM guarantors |
| Data size | PoV up to ~10 MB | PoV up to ~15 MB (larger Refine budget) |

### 7.2 Collator Discovery

Collators must discover which validator sub-group (guarantors) is assigned to their parachain's
core for the current slot. This information is available on-chain in JAM's core assignment
state, and collators can subscribe to it via the same validator discovery mechanisms used today.

### 7.3 Multiple Blocks Per Slot (Elastic Scaling)

JAM's model is naturally compatible with elastic scaling. When a parachain holds multiple
cores, it can submit a chain of dependent candidates (block N, N+1, N+2, …) as separate
work items across those cores.

Because Refine is stateless — each work item carries its own `PersistedValidationData`
(parent head, state root) and PoV — JAM can validate all candidates in the chain **in
parallel** across cores without knowing about their dependency. No special JAM-level
orchestration is needed.

The ordering constraint is enforced entirely by the **Parachain Service's Accumulate**:
it applies candidates sequentially (N before N+1 before N+2) and rejects later candidates
if an earlier one in the chain fails availability or validation. This keeps the dependency
logic where it belongs — in the service, not in JAM's core protocol.

---

## 8. Messaging & XCM

### 8.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, with a practical limit of ~1 MB per channel per block. UMP (Upward
Message Passing) similarly routes messages from parachain to relay chain.

### 8.2 Full XCMP on JAM

JAM enables **full XCMP** (the original design): only message *headers* and *hashes* are
recorded on-chain; the actual message payloads are distributed off-chain via JAM's data
availability system. This removes the per-message size bottleneck.

For the Parachain Service:

- **Outgoing HRMP messages** from a parachain are included in `CandidateCommitments.horizontal_messages`.
  Their payload hashes are recorded in the Parachain Service state; payloads are available
  via JAM DA.
- **Incoming HRMP messages** are fetched by the recipient parachain's collators from JAM DA,
  keyed by the message hash recorded on-chain.

### 8.3 UMP / DMP Replacement

Upward messages (parachain → relay chain) and downward messages (relay chain → parachain) are
replaced by the **inter-service messaging** mechanism: JAM's `OnTransfer` allows the Parachain
Service to send messages (with funds) to other services (e.g. Asset Hub). Similarly, other
services can trigger actions in the Parachain Service by sending it a transfer with a memo.

The mapping:

| Polkadot 1.x | JAM Parachain Service |
|-------------|----------------------|
| UMP (upward messages) | Work result commitments processed by Accumulate; may trigger `OnTransfer` to another service |
| DMP (downward messages) | `OnTransfer` from another service into the Parachain Service, queued for the next parachain candidate |
| HRMP | Full XCMP via JAM DA |

### 8.4 XCM Versions

XCM itself is unaffected by this migration at the message format level. The underlying transport
changes (HRMP → XCMP, UMP/DMP → OnTransfer), but XCM programs continue to be expressed and
routed using existing XCM versions.

---

## 9. Migration Path

### 9.1 Phased Approach

Migration from Polkadot 1.x to the JAM Parachain Service is envisioned as phased:

**Phase 0 — Compatibility Layer**  
The Parachain Service initially provides a compatibility shim: existing Wasm-based parachains
continue to work. The PVF is wrapped: the Refine function compiles and runs the Wasm PVF via a
PVM-hosted Wasm interpreter, avoiding immediate recompilation requirements.

**Phase 1 — PVM-native PVFs**  
Parachain teams recompile their runtimes targeting RISC-V/PVM. The Parachain Service
supports both Wasm and PVM PVFs simultaneously during a transition window.

**Phase 2 — Full JAM Model**  
All parachains run PVM-native PVFs. The Wasm compatibility layer is removed. Full XCMP is
enabled. Coretime allocation flows directly through JAM's coretime markets.

### 9.2 State Migration

Parachain state (head data, HRMP channels, code, etc.) currently lives in the relay chain
runtime storage. It must be migrated into the Parachain Service's key-value store when JAM
launches.

This migration is a one-time operation performed at the JAM genesis block, reading relay chain
state and initialising the Parachain Service accordingly.

### 9.3 Collator Migration

Collators require no significant code changes in Phase 0. In later phases, they need to:

- Target JAM's networking layer for work package submission (rather than the relay chain p2p).
- Understand the new work item format.
- Support the larger PoV budget.

Cumulus-based parachains will receive SDK support for this migration.

### 9.4 Ecosystem Tooling

Downstream tooling (block explorers, indexers, wallets) is largely unaffected in Phase 0, since
the parachain-visible API (finality, head data, XCM) remains the same. Later phases may require
updates to understand the new DA-based message model.

---

## 10. Open Questions

The following questions are not yet resolved and require further design work or community input:

1. **Coretime assignment bridging**: How exactly does the coretime chain communicate assignments
   to the Parachain Service? Options include: (a) an on-chain message via `OnTransfer`, (b) a
   dedicated JAM extrinsic type for service configuration, or (c) making the Parachain Service
   itself the coretime chain.

2. **Accumulate gas budget**: Is ~10ms sufficient for all the Accumulate logic (head update,
   message processing, code upgrades)? The current relay chain performs more work on inclusion
   than this budget allows. Mitigation strategies (batching, deferral) need to be specified.

3. **Dispute integration**: JAM's judgment mechanism operates at the work-report level, not the
   individual-candidate level. How does the Parachain Service's per-para dispute semantics map
   onto JAM's per-work-report judgment? What happens if one candidate in a multi-candidate work
   report is disputed?

4. **PVF preimage lifecycle**: The current relay chain has a governance-managed PVF pre-checking
   process. How does the Parachain Service manage PVF registration and the lookup-anchor lifecycle
   (requesting preimage availability, expiring old PVFs)?

5. **HRMP channel open/close during transition**: Channel management today requires relay chain
   extrinsics with deposits. How is channel management expressed in the JAM model? Does it
   become a Parachain Service-internal operation, or do parachains submit channel management
   requests via their own UMP/work-item messages?

6. **Parachain registration & governance**: Today, parachains are registered via on-chain
   governance (Polkadot OpenGov). In JAM, what is the registration mechanism for the Parachain
   Service? Is it a `ServiceCreation` operation, or does the Parachain Service maintain its own
   internal governance?

7. **Wasm compatibility layer performance**: A PVM-hosted Wasm interpreter will have significant
   overhead. Is it acceptable as a Phase 0 compatibility measure, or do we need LLVM-based
   Wasm→RISC-V AOT compilation available from day one?

8. **Finality guarantees during the judgment window**: Polkadot parachains currently offer
   ~1-minute finality. JAM's 1-hour judgment window means finality of parachain blocks is
   technically delayed. How do parachains and ecosystem tooling communicate this changed
   finality model?

---

## 11. References

- [JAM Gray Paper](https://graypaper.com) — Formal JAM specification (Gavin Wood)
- [CoreJAM RFC #31](https://github.com/polkadot-fellows/RFCs/pull/31) — Original CoreJAM RFC
- [RFC-1: Agile Coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md)
- [RFC-5: Coretime Interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md)
- [Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
- [Polkadot Wiki: JAM Chain](https://wiki.polkadot.network/docs/learn-jam-chain)
- [Demystifying JAM](https://blog.kianenigma.com/posts/tech/demystifying-jam/) — Kian Paimani


# TODO

- How to support loading data preimages from JAM?
  - Probably extend PoV to expose the required inputs
