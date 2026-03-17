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
   - 5.3 [Code Upgrade Lifecycle](#53-code-upgrade-lifecycle)
6. [Authorization & Coretime](#6-authorization-coretime)
7. [Messaging](#7-messaging)
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

The Parachain Service maps the current relay chain's parachain host logic onto JAM's
two execution domains:

- **Refine (in-core)**: Executes `validate_block` — the PVF validation that backing
  validators currently perform. Guarantors run the PVF against the PoV to verify the
  parachain block candidate. This replaces the current backing subsystem.
- **Accumulate (on-chain)**: Performs candidate enactment — updating head data, processing
  signals, managing channels and code upgrades. This replaces the current inclusion pallet
  logic (`enact_candidate`).

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
              It records the new parachain head, processes upward signals (code upgrades,
              HRMP channel management, outbound transfers), updates XCMP channel metadata,
              and queues downward signals for parachains.

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
| Coretime accounting | `coretime` + `on_demand` pallets | Core assignment + usage tracking | **JAM native** |
| Data availability | Availability distribution subsystem | Erasure-coded DA layer | **JAM native** |

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

    /// Channel state (open channels, message metadata).
    /// The exact structure depends on the final XCMP design — with full XCMP,
    /// only message hashes and channel metadata are stored on-chain, not payloads.
    channels: Map<ChannelId, Channel>,

    /// Downward signal queues (from Parachain Service → parachain).
    dsq: Map<ParaId, VecDeque<DownwardSignal>>,

    /// PVF (Parachain Validation Function) preimage registry.
    /// Maps validation_code_hash → (code, ref_count, expiry).
    pvf_registry: Map<ValidationCodeHash, PvfEntry>,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Hash of the currently active validation code.
    validation_code_hash: ValidationCodeHash,
    /// Pending code upgrade, if any. See §5.3 for the full lifecycle.
    pending_upgrade: Option<PendingCodeUpgrade>,
}

/// Note: the service state should also maintain a reverse index from
/// deadline timeslot to ParaId, so expired upgrades can be efficiently
/// cleaned up during Accumulate.
struct PendingCodeUpgrade {
    /// Hash of the new PVM code.
    new_code_hash: ValidationCodeHash,
    /// Deadline: upgrade rejected if preimage is not available or no block
    /// uses the new code by this timeslot.
    deadline: Timeslot,
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

    /// The hash of the currently active validation code. Used by Refine to
    /// look up the PVF bytecode from the preimage store.
    validation_code_hash: ValidationCodeHash,

    /// The Proof-of-Validity (PoV) — the actual block data + witness.
    /// This is the large input to Refine (up to ~15 MB per slot across all items).
    pov: Vec<u8>,

    /// The work package's `RefineContext` provides the JAM block context (anchor hash,
    /// state root, beefy root) needed by the PVF. These are verified on-chain when the
    /// work report is submitted, so they do not need to be included in the work item.
}
```

Initially, each work package will contain a single work item (one parachain candidate).
Support for multiple items per package may be added later.

### 3.3 Work Reports

After the Refine step, guarantors produce a **work result** per work item. The work result
is an opaque output blob (up to ~48 KB shared with the authorizer trace, see W_R in the
Gray Paper). For the Parachain Service, this blob encodes the validated candidate outputs:

```rust
/// Encoded as the Refine output blob for one parachain candidate.
struct ParachainWorkResult {
    /// New head data produced by the parachain block.
    head_data: HeadData,
    /// Upward signals from the parachain (code upgrades, channel ops, transfers).
    upward_signals: Vec<UpwardSignal>,
    /// Hashes of outbound XCMP messages (payloads are exported to DA via export()).
    outbound_message_hashes: Vec<(ParaId, Hash)>,
    /// Number of downward signals processed by the parachain.
    processed_downward_signals: u32,
    /// HRMP watermark — up to which point inbound messages were consumed.
    hrmp_watermark: Timeslot,
}
```

The `ParaId` does not need to be in the work result — it is conveyed via the **authorizer
trace**, which the authorizer returns upon successful authorization and which is available
to Accumulate as part of the operand tuple.

The work report itself is subject to JAM's **guarantee** and **assurance** mechanisms — validators
attest its correctness and data availability is enforced before Accumulate runs.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

The Refine entry point of the Parachain Service executes the **Parachain Validation Function
(PVF)** for each work item. This is a stateless, in-core computation performed by guarantors
(validators assigned to the relevant JAM core).

Refine:
1. Fetches the PVF bytecode via `historical_lookup` (using `validation_code_hash`).
2. Instantiates a child PVM with the PVF.
3. Executes the PVF against the PoV (the `validate_block` call).
4. Returns a `ParachainWorkResult` with the committed outputs.

Because Refine is stateless, it cannot write to service storage. The only "statefulness" it can
exercise is via preimage lookups — which is exactly how PVF code is accessed.

### 4.2 PVF Execution in PVM

PVFs execute in the **Polkadot Virtual Machine (PVM)**, a RISC-V based VM. The resources
available to PVF execution during Refine:

- **Gas**: Up to 6 seconds of PVM gas per Refine invocation (one full JAM slot).
- **Memory**: Up to 4 GB addressable memory (PVM is 64-bit RV64, but PVF memory is capped).
- **Code size**: Up to W_C = 4 MB for the PVF bytecode.
- **I/O**: Access to the work item payload (PoV), import segments from the DA layer, and
  preimage lookups from the service's preimage store. Outbound data can be written to DA
  segments via `export()`.

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

3. **Block context**: The relay-parent context (header hash, state root, etc.) needed
   by the PVF comes from the work package's `RefineContext` — a
   JAM-native struct containing `anchor` (recent header hash), `state_root`, `beefy_root`,
   `lookup_anchor`, and `lookup_anchor_slot`. These values are set by the work package
   builder (collator) and **verified on-chain** when the work report is submitted, so
   Refine can trust them without additional validation.

4. **Import segments**: JAM work items may reference **import segments** — data blobs from
   the JAM Data Lake that are made available to Refine via the import manifest. The
   Parachain Service can use import segments to provide additional context data (e.g.,
   recent relay chain headers) without including them in the PoV.

```
Work Package
├── Work Item (ParachainWorkItem)
│   ├── payload: PoV                                ← inline, via work_item_payload()
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
  Like crypto operations, benchmarking is needed to quantify PVM-compiled performance vs
  native — though hashing is generally less sensitive to overhead than big-integer crypto.
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
- **Data export** (`export`): Writing to the JAM Data Lake. The Refine function uses
  `export()` to write outbound XCMP message payloads into DA segments. Recipient
  parachains' collators can then fetch these segments directly from the DA layer by
  segment hash, avoiding the need to route full message payloads through on-chain state.
  Accumulate only records the message hashes and channel metadata — the payloads remain
  off-chain in the DA layer (see §7.2).
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
2. **Upward signal processing**: Processes `UpwardSignal`s from the candidate commitments:
   - `RequestCodeUpgrade` — calls `solicit(new_code_hash, code_len)` to request the
     preimage via the JAM preimage store, sets `pending_upgrade` with a deadline, and
     begins the dual-code transition period. See §5.3 for the full lifecycle.
   - `InitOpenChannel` / `AcceptOpenChannel` / `CloseChannel` — manages HRMP channel
     lifecycle and queues corresponding `DownwardSignal`s to the affected parachains.
   - `TransferOut` — calls `transfer()` to the destination JAM service (e.g. Asset Hub).
3. **Downward signal pruning**: Removes consumed downward signals from the parachain's
   signal queue, based on the `processed_downward_signals` count in the candidate commitments.
4. **HRMP watermark advancement**: Prunes inbound HRMP messages up to the watermark declared
   in the candidate commitments, freeing channel capacity.
5. **Outbound HRMP queuing**: Enqueues horizontal messages from the candidate commitments into
   the destination parachain's HRMP channel, updating channel metadata (message count, total
   size, MQC head hash).
6. **Code upgrade transition check**: Checks whether the candidate was validated with
   a pending new PVF code. If so, activates the new code (updates
   `ParaInfo.validation_code_hash`), calls `forget()` on the old code hash, and
   clears `pending_upgrade`. Also checks if a pending upgrade has exceeded its deadline
   without the preimage becoming available or without any block using the new code — if
   so, rejects the upgrade by calling `forget()` on the new code hash and clearing
   `pending_upgrade`.
7. **Incoming transfer processing**: Any `OnTransfer` calls received from other JAM services
   are decoded (memo identifies the target `ParaId`) and queued as `DownwardSignal::IncomingTransfer`
   for the destination parachain.

Compared to the relay chain's inclusion pallet, the Accumulate function is notably simpler.
Several responsibilities that the relay chain handles are instead managed by JAM natively:

- **Availability tracking**: JAM's guarantee and assurance mechanism tracks availability
  votes and determines when a candidate is available — the Parachain Service does not need
  to maintain `PendingAvailability` state or process availability bitfields.
- **Dispute resolution**: JAM's judgment mechanism handles dispute escalation, slashing,
  and finality delays at the protocol level. The Parachain Service only needs to handle
  rollback if notified of an invalid judgment (handled by JAM's native judgment mechanism).
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
### 5.3 Code Upgrade Lifecycle

Runtime (PVF) code upgrades follow a well-defined lifecycle using JAM's preimage
store (`solicit`/`provide`/`forget`) and the `xtpreimages` block extrinsic.

```
Phase 1: Request
  Parachain includes UpwardSignal::RequestCodeUpgrade { new_code_hash }
  in its candidate commitments.
      │
      ▼
Phase 2: Rquest Preimage
  Accumulate calls solicit(new_code_hash, code_len).
  Sets pending_upgrade with a deadline (current timeslot + UPGRADE_TIMEOUT).
  The parachain now pays for TWO PVF codes in the preimage store.
      │
      ▼
Phase 3: Preimage Submission
  Anyone (collator, block author, third party) can submit the PVM code directly to JAM.
      │
      ▼
Phase 4: Transition Period
  Once the preimage is available, collators MAY build blocks using either
  the old or the new PVF code. Refine accepts both validation_code_hash
  and pending_upgrade.new_code_hash during this window.
  The parachain runtime itself can check preimage availability (via the
  Parachain Service state exposed through the validation inputs) and
  trigger the switch from within its own block execution — no
  service-side polling is needed.
      │
      ▼
Phase 5: Activation or Rejection
  (a) First block using new code: Accumulate detects the candidate was
      validated with new_code_hash. It:
      - Sets validation_code_hash = new_code_hash
      - Calls forget(old_code_hash, old_code_len) to release the old code
      - Clears pending_upgrade
      The transition is complete. Only the new code is accepted from now on.

  (b) Deadline exceeded: If the deadline (set in Phase 2) passes without
      the preimage becoming available or without any block using the new
      code, Accumulate rejects the upgrade:
      - Calls forget(new_code_hash, code_len) to release the new code
      - Clears pending_upgrade
      The parachain continues with the old code.
```

**Key properties:**

- **No pre-checking needed**: PVM has no compilation bomb risk (unlike WASM), so there is
  no pre-checking vote. The code is accepted as soon as the preimage is available.
- **Dual-code cost**: During the transition period, the parachain pays for both the old
  and new PVF code in the preimage store. This incentivizes timely adoption.
- **Permissionless submission**: The preimage can be submitted by anyone — the collator,
  block author, or any third party. The JAM protocol validates the hash against the
  solicitation.
- **Timeout protection**: The deadline prevents parachains from indefinitely occupying
  preimage store space with unused code. `UPGRADE_TIMEOUT` should be long enough for
  collators to update their software (e.g. 24-48 hours).
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

JAM natively manages core assignment and tracks core usage. Parachains obtain coretime from
the **coretime chain** (currently `pallet-broker` on the Coretime system chain). The coretime
allocation pipeline remains largely unchanged, but the destination of the assignment changes
from "relay chain availability cores" to "JAM service cores".

The Parachain Service provides an **authorizer** — a piece of PVM code that JAM calls to
validate each work package before guarantors execute Refine. The authorizer checks that the
submitting parachain holds valid coretime for the requested core:

```
Coretime Chain (pallet-broker)
    │  assigns coretime to para_id on core N
    ▼
JAM (authorizer invocation)
    │  is_authorized() validates coretime assignment for work package
    ▼
Guarantors
    │  execute Refine for para_id's work items on core N
    ▼
JAM chain (Accumulate)
```

The authorizer's state (valid coretime assignments per parachain) is maintained by the
Parachain Service's Accumulate function, updated when coretime assignments arrive from the
Coretime Chain.

### 6.3 On-Demand Parachains


On-demand parachains (currently acquired via `pallet-on-demand`) continue to work: they
acquire a single-shot coretime allocation for one slot, submitted as a work package to the
Parachain Service for that slot only. This can be handled via JAM's own bulk/on-demand coretime
markets or via the Parachain Service's own internal priority queue.

---

## 7. Messaging

### 7.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, with a practical limit of ~1 MB per channel per block. UMP (Upward
Message Passing) similarly routes messages from parachain to relay chain.

### 7.2 Full XCMP on JAM

The current HRMP model — routing full message payloads through the relay chain — cannot
work on JAM because the work result output is too small to carry message payloads on-chain.
Off-chain messaging is required.

JAM enables **full XCMP** (the original design): only message *headers* and *hashes* are
recorded on-chain; the actual message payloads are distributed off-chain via JAM's data
availability layer (D3L). This removes the per-message size bottleneck. The Refine function
uses `export()` to write outbound message payloads into DA segments, and Accumulate only
records the message hashes and channel metadata on-chain.

This also enables **speculative messaging**: since message payloads are in DA segments
(available for 28 days), recipient parachains can optimistically consume messages before
they are finalized, as proposed in the speculative messaging design.

For the Parachain Service:

- **Outgoing XCMP messages** from a parachain are included in the candidate commitments.
  During Refine, their payloads are exported to DA segments via `export()`. Accumulate
  records the payload hashes and updates channel metadata in the Parachain Service state.
- **Incoming XCMP messages** are fetched by the recipient parachain's collators directly
  from the DA layer by segment hash, keyed by the message hash recorded on-chain.

### 7.3 Upward & Downward Signals (UMP / DMP Replacement)

In Polkadot 1.x, UMP and DMP carry XCM programs between parachains and the relay chain.
On JAM, these are replaced by a **typed signal protocol** — no XCM is involved. Signals
are simple, well-defined message types that the Parachain Service processes directly.

#### Upward Signals (Parachain → Parachain Service)

Upward signals are included in the parachain's `CandidateCommitments` and processed by
Accumulate. They replace UMP's current role as a general-purpose XCM transport.

```rust
enum UpwardSignal {
	/// Request a runtime (PVF) code upgrade. Only the code hash is included.
	/// Accumulate calls `solicit(new_code_hash, code_len)` to request the preimage
	/// via the JAM preimage store. Anyone can then submit the code blob as a
	/// `xtpreimages` block extrinsic. See §5.3 for the full upgrade lifecycle.
	RequestCodeUpgrade { new_code_hash: ValidationCodeHash },
	/// Request to open an HRMP channel to another parachain.
	InitOpenChannel { recipient: ParaId, max_capacity: u32, max_message_size: u32 },
	/// Accept a pending HRMP channel open request.
	AcceptOpenChannel { sender: ParaId },
	/// Request to close an existing HRMP channel.
	CloseChannel { sender: ParaId, recipient: ParaId },
	/// Transfer DOT to Asset Hub. Accumulate calls `transfer()` to the
	/// Asset Hub service with the memo as payload. Only Asset Hub is a valid
	/// destination — parachain-to-parachain DOT transfers are handled
	/// internally by Asset Hub, not by the Parachain Service.
	TransferToAssetHub { amount: Balance, memo: Vec<u8> },
	/// Update the authorizer queue for a core. Only callable by the Coretime
	/// Chain. Accumulate calls the JAM `assign` host call (ΩA) to set the
	/// authorizer queue for the given core. The `assign` host call takes a
	/// core index, a sequence of authorizer code hashes (the queue), and
	/// validates that the calling service is assigned to that core.
	SetAuthorizerQueue { core: CoreIndex, authorizers: Vec<AuthorizerHash> },
}
```

#### Downward Signals (Parachain Service → Parachain)

Downward signals are queued in the Parachain Service state and delivered to the
parachain as part of its next candidate's input data (replacing the DMP queue).

```rust
enum DownwardSignal {
	/// Notification that another parachain wants to open an HRMP channel.
	HrmpOpenRequest { sender: ParaId, max_capacity: u32, max_message_size: u32 },
	/// Notification that the recipient accepted the HRMP channel.
	HrmpAccepted { recipient: ParaId },
	/// Notification that a channel is being closed.
	HrmpClosing { initiator: ParaId, sender: ParaId, recipient: ParaId },
	/// DOT received from an external JAM service (e.g. Asset Hub) via
	/// `OnTransfer`. The source is always an external service, never another
	/// parachain — inter-parachain transfers are handled by Asset Hub directly.
	IncomingTransfer { source: ServiceId, amount: Balance, memo: Vec<u8> },
}
```

### 7.4 XCM

XCM is **not used** for upward/downward signaling between parachains and the Parachain
Service. XCM remains the message format for **inter-parachain** communication via XCMP
(§7.2) — parachain-to-parachain messages continue to be XCM programs. The transport
changes (HRMP → full XCMP via DA), but the XCM format itself is unaffected.

---

## 8. Open Questions

The following questions are not yet resolved and require further design work or community input:

1. **Coretime and authorization**: JAM natively handles core assignment and coretime tracking.
   The Parachain Service provides an **authorizer** — service-specific code that JAM calls
   to validate work packages before Refine. The authorizer verifies that the submitting
   parachain holds valid coretime for the requested core. The exact interface between the
   Coretime Chain and the Parachain Service's authorizer state needs to be specified.

2. **Accumulate gas budget**: Is ~10ms sufficient for all the Accumulate logic (head update,
   message processing, code upgrades)? The current relay chain performs more work on inclusion
   than this budget allows. Mitigation strategies (batching, deferral) need to be specified.

3. **Dispute integration**: JAM's judgment mechanism operates at the work-report level, not the
   individual-candidate level. How does the Parachain Service's per-para dispute semantics map
   onto JAM's per-work-report judgment? What happens if one candidate in a multi-candidate work
   report is disputed?

4. **`UPGRADE_TIMEOUT` value**: The code upgrade lifecycle (§5.3) requires a timeout after
   which an unused upgrade is rejected. The appropriate value depends on the expected
   collator update cadence and preimage store cost model. Candidates: 24-48 hours.

5. **Parachain registration & governance**: Today, parachains are registered via on-chain
   governance (Polkadot OpenGov). In JAM, what is the registration mechanism for the Parachain
   Service? Is it a `ServiceCreation` operation, or does the Parachain Service maintain its own
   internal governance?

6. **Finality guarantees during the judgment window**: Polkadot parachains currently offer
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


