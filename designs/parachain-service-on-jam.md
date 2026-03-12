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
   - 4.1 [What Refine Does](#41-what-refine-does)
   - 4.2 [PVF Execution in PVM](#42-pvf-execution-in-pvm)
   - 4.3 [Guarantor Assignment](#43-guarantor-assignment)
   - 4.4 [Preimage Access & Data Loading](#44-preimage-access-data-loading)
   - 4.5 [Host Functions & PVM Imports](#45-host-functions-pvm-imports)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
6. [Authorization & Coretime](#6-authorization-coretime)
7. [Messaging & XCM](#7-messaging-xcm)
8. [Open Questions](#8-open-questions)
9. [References](#9-references)

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
- Cross-chain messaging under the new model

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
[Collect]     Collator gathers transactions, builds a parachain block candidate and puts it into a work package.
    │
    ▼
[Refine]      IN-CORE: Guarantors execute the parachain service refine functionality. Internally this calls the PVF code to verify the work package.
              Stateless, off-chain, metered via PVM gas.
              Output: a compact Work Result (~90 kB) summarising the validated candidate.
    │
    ▼
[Join]        The Work Report (aggregating Work Results) is submitted on-chain.
              JAM validators attest (guarantee) its correctness. Availability is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate function runs on-chain.
              It records the new parachain head, processes message hashes (with full XCMP,
              message payloads are off-chain via JAM DA — Accumulate only checks hashes
              and updates channel metadata), handles code upgrades, and updates coretime
              accounting.

```

### Responsibility Split: JAM Native vs. Parachain Service

A key aspect of the migration is that many relay chain responsibilities are absorbed by JAM
natively. The Parachain Service only implements what is specific to the parachain protocol:

| Responsibility | Polkadot 1.x (Relay Chain) | JAM | Owner |
|---|---|---|---|
| PVF execution (validation) | Backing subsystem + PVF executor | Refine entry point executes PVF in child PVM | **Parachain Service** |
| Candidate availability tracking | `inclusion` pallet (bitfields, thresholds) | Guarantee + Assurance mechanism | **JAM native** |
| Approval voting | Approval voting subsystem | Auditing + assurance protocol | **JAM native** |
| Dispute resolution | `disputes` pallet + dispute coordinator | Judgment mechanism | **JAM native** |
| Head data & state tracking | `inclusion` + `paras` pallets | Accumulate entry point | **Parachain Service** |
| Code upgrades | `paras` pallet (PVF pre-checking) | Accumulate + preimage store | **Parachain Service** |
| Coretime accounting | `coretime` + `on_demand` pallets | Accumulate records core usage | **Parachain Service** |
| Data availability | Availability distribution subsystem | Erasure-coded DA layer | **JAM native** |

---

TODO: I havve doubts that the parachain service will be responsible for the coretime accounting, this  should be JAM native?

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

    /// HRMP channel state (open channels, queued messages).
    hrmp_channels: Map<HrmpChannelId, HrmpChannel>,

    /// Downward message queues (from Parachain Service → parachain).
    dmq: Map<ParaId, VecDeque<DownwardMessage>>,

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
}
```

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
    pov: Vec<u8>,

    /// JAM block context needed by the PVF: block hash, block number, and state root.
    /// Assembled by the collator from on-chain Parachain Service state.
    /// TODO: We can not pass them here, we need to verify them. Check on what we have available in `refine`.
    jam_block_hash: Hash,
    jam_block_number: u32,
    jam_state_root: Hash,
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

### 4.3 Guarantor Assignment

Guarantors are assigned to cores by JAM's core assignment mechanism. The Parachain Service
does not manage this directly — JAM natively handles core-to-validator-group mapping. For
each slot, JAM assigns a small validator sub-group to each core. This mirrors the current
"backing group" model but is driven by JAM's generic core assignment rather than a
parachain-specific scheduler. See §6 for how coretime allocation feeds into this.

### 4.4 Preimage Access & Data Loading

Refine is stateless — it cannot directly access service storage. However, JAM provides a
**preimage lookup** mechanism that allows Refine to fetch data blobs by hash. This is the
primary mechanism the Parachain Service uses to load PVF code and other data needed for
validation.

The data loading pipeline for a parachain candidate works as follows:

1. **PVF code**: The parachain's validation function bytecode is stored as a preimage in
   the Parachain Service's preimage store. The Accumulate function registers PVF code via
   `solicit()` + `provide()` when a parachain first registers or schedules a code upgrade.
   During Refine, the guarantor calls `lookup(validation_code_hash)` to fetch the PVF
   bytecode from the preimage store.

2. **Proof-of-Validity (PoV)**: The PoV is the large input (~15 MB budget) containing the
   parachain block data and state witness. It is carried **inline** as part of the work
   item payload, accessed via `work_item_payload()`. This data is not a preimage — it is
   submitted fresh with each work package.

3. **Persisted validation data**: The relay-parent context (state root, block number, etc.)
   needed by the PVF is included in the work item. In Polkadot 1.x this is constructed
   from relay chain state; in JAM it is assembled by the collator from Parachain Service
   state available on-chain and included directly in the work package.

4. **Import segments**: JAM work items may reference **import segments** — data blobs from
   the JAM Data Lake that are made available to Refine via the import manifest. The
   Parachain Service can use import segments to provide additional context data (e.g.,
   recent relay chain headers) without including them in the PoV.

```
Work Package
├── Work Item (ParachainWorkItem)
│   ├── payload: PoV + PersistedValidationData     ← inline, via work_item_payload()
│   ├── import manifest: [segment_hash, ...]        ← optional, via import()
│   └── service: Parachain Service ID
│
└── Refine execution
    ├── lookup(validation_code_hash) → PVF bytecode ← from preimage store
    ├── work_item_payload() → PoV                   ← inline data
    ├── import(0) → additional context               ← from Data Lake (if needed)
    └── machine() + invoke() → execute PVF           ← child PVM instance
```

This design means the PoV format is largely unchanged from Polkadot 1.x — it still contains
the parachain block data and state witness proofs. The key difference is that PVF code is
accessed via preimage lookup rather than being part of the validator's local state.

### 4.5 Host Functions & PVM Imports

In Polkadot 1.x, PVFs execute in a WebAssembly sandbox with a restricted set of **host
functions** provided by the validator. On JAM, PVFs execute inside a child PVM instance
spawned by the Parachain Service's Refine function. This fundamentally changes which
operations require host function support versus being compiled directly into PVM guest code.

#### Current PVF Host Functions (Polkadot 1.x)

The current PVF executor exposes exactly six categories of host functions:

| Category | Examples | Purpose |
|----------|----------|---------|
| **Crypto** | `sr25519_verify`, `ed25519_verify`, `secp256k1_ecdsa_recover` | Signature verification |
| **Hashing** | `blake2_256`, `keccak_256`, `twox_128` | Data integrity |
| **Trie** | `blake2_256_root`, `blake2_256_verify_proof` | State proof verification |
| **Allocator** | `malloc`, `free` | Wasm memory management |
| **Logging** | `log`, `max_level` | Debug output |
| **Misc** | `print_num`, `print_utf8` | Debug output |

Notably excluded: all storage operations, offchain workers, tracing, and transaction indexing.
PVF execution is fully stateless — state verification happens via trie proofs, not storage
reads.

#### PVM Transition: What Changes

On JAM, most of these host functions can be **eliminated** because their implementations can
be compiled directly into PVM guest code (RISC-V). The key categories:

**Can be compiled into guest code (no host call needed):**

- **Hashing** (`blake2_256`, `keccak_256`, `sha2_256`, `twox_*`): Pure computation.
  These algorithms compile straightforwardly to RISC-V and can run as ordinary guest code.
  TODO: We also need benchmarking for this.
- **Trie operations** (`blake2_256_root`, `blake2_256_verify_proof`): Built on top of
  hashing — once hashing is native, trie operations are too.
- **Allocator** (`malloc`, `free`): PVM uses a RISC-V memory model where the guest manages
  its own heap. No host-provided allocator is needed.
- **Misc / Logging**: Logging can use PVM's native `log` host call (JIP-1); print functions
  are unnecessary.

**Crypto operations — compiled with potential host call acceleration:**

- **Signature verification** (`sr25519_verify`, `ed25519_verify`, `ecdsa_*`): These are
  pure computation and *can* be compiled to RISC-V. However, compiled crypto in PVM is
  expected to be slower than native execution. The performance gap depends on how well
  the PVM JIT compiler handles big-integer arithmetic and field operations.
- Benchmarking is needed to quantify the overhead of PVM-compiled crypto versus native
  host functions. If the overhead is acceptable (e.g., 2-3x), crypto can be fully compiled.
  If it is prohibitive (e.g., 10x+), the Parachain Service's Refine function may provide
  crypto verification as host calls to the child PVM executing the PVF.
- Note that PVM's JIT compilation to native RISC-V (or x86/ARM via transpilation) may
  narrow this gap significantly compared to interpreted execution.

**Must remain as host calls (JAM-provided):**

- **Preimage lookup** (`lookup`, `foreign_lookup`): Accessing the JAM preimage store
  requires interaction with the JAM runtime — this cannot be compiled into guest code.
- **Data export** (`export`): Writing to the JAM Data Lake for XCMP message payloads.
  TODO: We need to think how we can use this for messages. I think we can use the DA layer to directly just fetch these individual segments. If yes, we could use this to store the messages for others to fetch.
- **Gas metering** (`gas`): Inspecting the remaining gas budget.

#### Practical Architecture

The Parachain Service's Refine function acts as a **shim** between JAM's host call
interface and the PVF's expected environment:

```
JAM Host Calls                Parachain Service Refine           Child PVM (PVF)
─────────────────────────     ──────────────────────────         ─────────────────
lookup(hash)           ←──── fetches PVF code               ──→ PVF bytecode loaded
work_item_payload()    ←──── extracts PoV                   ──→ PoV passed as input
machine() + invoke()   ←──── creates child PVM              ──→ PVF executes
                              provides crypto (compiled       ──→ calls crypto functions
                              or host-accelerated)                (linked or host-called)
export()               ←──── exports XCMP payloads
```

This shim pattern means the PVF itself does not need to know whether it is running on
Polkadot 1.x or JAM — the interface it sees (validation data in, commitments out) remains
the same. The differences are handled by the Parachain Service's Refine wrapper.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage, roughly ~10ms of PVM gas per work result.

Accumulate for the Parachain Service performs the following operations, in order. These
correspond directly to the relay chain's current `enact_candidate` logic in the inclusion
pallet (`polkadot/runtime/parachains/src/inclusion/mod.rs`):

1. **Head data update**: Writes the new `head_data` from the candidate commitments into
   `ParaInfo` for the parachain, and records the relay-parent context.
2. **Code upgrade scheduling**: If `new_validation_code` is present in the commitments,
   schedules a code upgrade. The new PVF code is registered in the preimage store via
   `solicit()` + `provide()`, and a cooldown period is recorded to prevent concurrent
   upgrades. Code activation is deferred to a future Accumulate call once the soaking
   period has elapsed.
3. **Downward message pruning**: Removes consumed downward messages from the parachain's
   DMP queue, based on the `processed_downward_messages` count in the candidate commitments.
4. **Upward message reception**: Enqueues upward messages (UMP) from the parachain into
   the Parachain Service's processing queue. These may trigger `transfer()` calls to other
   JAM services (replacing the relay chain's UMP dispatch to `MessageQueue`).
5. **HRMP watermark advancement**: Prunes inbound HRMP messages up to the watermark declared
   in the candidate commitments, freeing channel capacity.
6. **Outbound HRMP queuing**: Enqueues horizontal messages from the candidate commitments into
   the destination parachain's HRMP channel, updating channel metadata (message count, total
   size, MQC head hash).
7. **Code activation check**: If a previously scheduled code upgrade's soaking period has
   elapsed, activates the new PVF code by updating `ParaInfo.validation_code_hash` and
   retiring the old code from the preimage store.
8. **Coretime accounting**: Records that the relevant core was used for this slot.

Compared to the relay chain's inclusion pallet, the Accumulate function is notably simpler.
Several responsibilities that the relay chain handles are instead managed by JAM natively:

- **Availability tracking**: JAM's guarantee and assurance mechanism tracks availability
  votes and determines when a candidate is available — the Parachain Service does not need
  to maintain `PendingAvailability` state or process availability bitfields.
- **Dispute resolution**: JAM's judgment mechanism handles dispute escalation, slashing,
  and finality delays at the protocol level. The Parachain Service only needs to handle
  rollback if notified of an invalid judgment (see §5.3).
- **Validator rewards**: Backing and availability attestation rewards are handled by JAM's
  validator reward mechanism, not by the Parachain Service.

In practice, the core Accumulate logic is primarily **parachain bookkeeping**: updating head
data, managing message queues, and tracking code upgrades. The heavy lifting of consensus
and security is delegated to JAM.

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
Parachain Service is an open question (see §8).

### 6.3 On-Demand Parachains

On-demand parachains (currently acquired via `pallet-on-demand`) continue to work: they
acquire a single-shot coretime allocation for one slot, submitted as a work package to the
Parachain Service for that slot only. This can be handled via JAM's own bulk/on-demand coretime
markets or via the Parachain Service's own internal priority queue.

---

## 7. Messaging & XCM

### 7.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, with a practical limit of ~1 MB per channel per block. UMP (Upward
Message Passing) similarly routes messages from parachain to relay chain.

### 7.2 Full XCMP on JAM

JAM enables **full XCMP** (the original design): only message *headers* and *hashes* are
recorded on-chain; the actual message payloads are distributed off-chain via JAM's data
availability system. This removes the per-message size bottleneck.

For the Parachain Service:

- **Outgoing HRMP messages** from a parachain are included in `CandidateCommitments.horizontal_messages`.
  Their payload hashes are recorded in the Parachain Service state; payloads are available
  via JAM DA.
- **Incoming HRMP messages** are fetched by the recipient parachain's collators from JAM DA,
  keyed by the message hash recorded on-chain.

### 7.3 UMP / DMP Replacement

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

### 7.4 XCM Versions

XCM itself is unaffected by this migration at the message format level. The underlying transport
changes (HRMP → XCMP, UMP/DMP → OnTransfer), but XCM programs continue to be expressed and
routed using existing XCM versions.

---

## 8. Open Questions

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

## 9. References

- [JAM Gray Paper](https://graypaper.com) — Formal JAM specification (Gavin Wood)
- [CoreJAM RFC #31](https://github.com/polkadot-fellows/RFCs/pull/31) — Original CoreJAM RFC
- [RFC-1: Agile Coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md)
- [RFC-5: Coretime Interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md)
- [Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
- [Polkadot Wiki: JAM Chain](https://wiki.polkadot.network/docs/learn-jam-chain)
- [Demystifying JAM](https://blog.kianenigma.com/posts/tech/demystifying-jam/) — Kian Paimani
- [JAM PVM Common API](https://docs.rs/jam-pvm-common/latest/jam_pvm_common/) — Host call specifications for Refine and Accumulate
- [JIP-1: Log Host Call](https://github.com/polkadot-fellows/JIPs/blob/main/JIP-1.md) — PVM logging specification


