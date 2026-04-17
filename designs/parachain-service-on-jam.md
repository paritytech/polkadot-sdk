# Parachain Service on JAM

> **Status**: Draft  
> **Last Updated**: 2026-03-02

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [The Parachain Service](#3-the-parachain-service)
   - 3.1 [Service State Layout](#31-service-state-layout)
   - 3.2 [Work Items](#32-work-items)
   - 3.3 [Refine Result](#33-refine-result)
4. [Refine: In-Core Execution](#4-refine-in-core-execution)
   - 4.1 [What Refine Does](#41-what-refine-does)
   - 4.2 [PVF Entry Point](#42-pvf-entry-point)
   - 4.3 [Host Functions & PVM Imports](#43-host-functions-pvm-imports)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
   - 5.1 [What Accumulate Does](#51-what-accumulate-does)
   - 5.2 [Code Upgrade Lifecycle](#52-code-upgrade-lifecycle)
6. [Parachain Registration](#6-parachain-registration)
7. [Authorization & Coretime](#7-authorization-coretime)
   - 7.3 [Authorizer Design: AURA Example](#73-authorizer-design-aura-example)
   - 7.4 [On-Demand Parachains](#74-on-demand-parachains)
8. [Messaging](#8-messaging)
9. [Missing JAM / Gray Paper Features](#9-missing-jam-gray-paper-features)
10. [References](#10-references)

---

## 1. Overview

This document describes the architecture of the **Parachain Service** — a JAM service that implements
Polkadot's parachain host functionality. The Parachain Service is the JAM successor to the current
Polkadot relay-chain parachain host, mapping all the concepts of collation, validation, availability,
and finality into JAM's Collect-Refine-Join-Accumulate (CRJA) computation model.

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
              Output: per-item work-digests plus authorization/export metadata, assembled into a Work Report.
    │
    ▼
[Join]        The Work Report (aggregating per-item work-digests and authorization metadata) is submitted on-chain.
              JAM validators attest (guarantee) its correctness. Availability is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate function runs on-chain.
              It records the new parachain head, applies the PVF's upward host-function
              effects (code upgrades, outbound transfers, authorizer updates), and queues
              incoming transfers from other services.

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

The Parachain Service is expected to be an **always-accumulate** service in the Gray Paper sense.
Even in blocks where no parachain candidate becomes available, it still needs an accumulation step
to apply privileged control-plane updates such as authorizer queue changes, validator-key updates,
and other service-level bookkeeping that must take effect without waiting for a parachain block.

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

    /// Incoming transfer queue for Asset Hub. Accumulate appends new
    /// transfers to the end; the PVF marks consumption via
    /// `consume_transfers_up_to(index)`. When the queue is fully consumed
    /// and empty, the index resets to 0.
    incoming_transfers: Vec<(ServiceId, Amount, Memo)>,

    /// Per-parachain error log. Stores the opaque error payload and
    /// authorizer trace from failed Refine executions, keyed by the JAM
    /// block timeslot at which Accumulate recorded them. Each parachain
    /// keeps at most 8 entries.
    error_log: Map<ParaId, CountedMap<Timeslot, ErrorEntry, 8>>,

    /// Pending authorizer queue updates that should be applied once the
    /// current 80-slot queue has been consumed.
    pending_authorizer_queues: Map<CoreIndex, Vec<AuthorizerHash>>,

    /// PVF (Parachain Validation Function) preimage registry.
    pvf_registry: Map<ValidationCodeHash, PvfEntry>,

    /// Stores when pending upgrades timeout.
    pending_upgrades_timeouts: Map<Timeslot, Vec<(ValidationCodeHash, ParaId)>>,
}

struct ErrorEntry {
    /// Opaque error payload from the PVF (max 1024 bytes).
    error_data: BoundedVec<u8, 1024>,
    /// Authorizer trace from the work-report.
    auth_trace: Vec<u8>,
}

struct PvfEntry {
	/// Length of the solicited preimage, used when forgetting it later.
	code_len: u32,
	/// Number of parachains currently referencing this validation code.
	/// When this drops to zero the preimage can be released immediately.
	ref_count: u32,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Hash of the currently active validation code.
    validation_code_hash: ValidationCodeHash,
    /// Pending code upgrade, if any: the new validation code hash and the
    /// deadline timeslot after which the upgrade is rejected. See §5.2.
    pending_upgrade: Option<(ValidationCodeHash, Timeslot)>,
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
}
```

Initially, each work package will contain a single work item (one parachain candidate).
Support for multiple items per package may be added later.

### 3.3 Refine Result

The Parachain Service's Refine function returns a parachain work result per work item. This result is forwarded to `accumulate`.
From the service's perspective, Refine either succeeds or fails:

```rust
/// The Parachain Service's Refine output for one parachain candidate.
/// Side effects from host functions (code upgrades, transfers, authorizer
/// updates) are recorded separately during Refine and forwarded to Accumulate.
enum ParachainWorkResult {
    Ok {
        /// New head data produced by the parachain block.
        head_data: Vec<u8>,
        /// Upward messages emitted through host functions during Refine.
        /// Accumulate replays these in order.
        upward_messages: Vec<UpwardMessage>,
    },
    /// PVF execution failed (e.g. invalid PoV, bad state proof, panic).
    ///
    /// The PVF may call `report_error(data)` to provide an opaque error payload
    /// (max 1024 bytes) before failing the execution.
    Err(BoundedVec<u8, 1024>),
}

enum UpwardMessage {
    RequestCodeUpgrade(ValidationCodeHash),
    TransferOut { dest: ServiceId, amount: Amount, memo: Memo },
    /// Set the authorizer queue
    ///
    /// - `immediate`: When set to `true`, the queue is overwritten immediately. Otherwise it waits until the current queue was processed.
    SetAuthorizerQueue { core: CoreIndex, queue: Vec<AuthorizerHash>, immediate: bool },
    SetValidatorKeys(Vec<ValidatorKey>),
}
```

The combined size of all result blobs plus the authorizer trace in a work-report is limited
to **48 KiB** by the Gray Paper.

- **`Ok`** is returned when PVF validation succeeds. The upward host-function calls made
  during Refine (code upgrades, transfers, authorizer updates, etc.) are carried alongside
  this result and applied by Accumulate.

- **`Err`** is returned when PVF validation fails. The PVF calls `report_error(data)` to
  supply an opaque error payload (up to 1024 bytes). Accumulate stores this payload together
  with the authorizer trace in the per-parachain error log (see §3.1), keyed by the
  timeslot of the JAM block in which Accumulate is running. The entry is deleted when a
  later successful candidate for the same parachain is accumulated **whose lookup-anchor
  timeslot is strictly greater than the stored entry's key**. This can be used for example
  to slash a collator who claimed an authorizer slot that was not theirs.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

The Refine entry point of the Parachain Service executes the **Parachain Validation Function
(PVF)** for each work item. This is a stateless, in-core computation performed by guarantors
(validators assigned to the relevant JAM core).

Refine:
1. Fetches the PVF bytecode via `historical_lookup` (using `validation_code_hash`).
2. Instantiates a child PVM with the PVF.
3. Executes the PVF against the PoV (the `jam_validate_block` call).
4. Assembles a `ParachainWorkResult` from the PVF's host-function side effects (see §4.2).

Because Refine is stateless, it cannot write to service storage.

### 4.2 PVF Entry Point

The Parachain Service's Refine spawns a child PVM and calls the PVF's single entry point:

```rust
fn jam_validate_block() -> ()
```

The PVF reads its inputs (PoV, context, downward transfers) through host functions and
writes its outputs (head data, code upgrades, transfers) through host functions. It does
not return a value directly — the `ParachainWorkResult` is assembled by the Parachain
Service's Refine wrapper from the accumulated host-function side effects.

If the PVF exits abnormally (panic, trap, or other failed execution), Refine treats this as
`Err` and records the opaque error payload previously supplied through `report_error(data)` if
one was provided.

### 4.3 Host Functions & PVM Imports

On JAM, PVFs execute inside a child PVM instance spawned by the Parachain Service's Refine
function. **Hashing**, **trie operations**, and **signature verification** are expected to
move into PVM guest code — transpilation to native code should bring acceptable performance,
though benchmarks are needed to confirm exact numbers.

#### Data access

These forward the full JAM fetch functionality to the PVF:

| Host function | Returns | Purpose |
|---|---|---|
| `lookup(hash: Hash)` | `Option<Vec<u8>>` | Fetch a preimage (e.g. PVF code) |
| `foreign_lookup(service: ServiceId, hash: Hash)` | `Option<Vec<u8>>` | Fetch a preimage from another service's store |
| `gas()` | `u64` | Query the remaining gas budget |
| `work_package()` | `WorkPackage` | Access the full encoded work package |
| `work_package_context()` | `RefineContext` | Access the refinement context (anchor, lookup-anchor, prerequisites) |
| `auth_config()` | `Vec<u8>` | Access the authorizer config blob |
| `auth_token()` | `Vec<u8>` | Access the authorization token blob |
| `work_items_summary()` | `Vec<WorkItemSummary>` | Summary of all work items (service, code hash, gas limits, counts, payload length) |
| `work_item_summary(index: u32)` | `Option<WorkItemSummary>` | Summary of a specific work item by index |
| `work_item_payload(index: u32)` | `Option<Vec<u8>>` | Payload of a specific work item by index |
| `import_segments()` | `Vec<SegmentMeta>` | Import segments metadata |
| `import_segment(index: u32)` | `Option<Vec<u8>>` | A specific import segment by index |

#### Side-effect host functions

These produce effects carried in the work result and applied by Accumulate:

| Host function | Returns | Purpose |
|---|---|---|
| `export(data: Vec<u8>)` | `u32` | Write a segment to the JAM Data Lake (e.g. outbound XCMP payloads). Returns segment index. |
| `request_code_upgrade(hash: ValidationCodeHash)` | `()` | Signal a PVF code upgrade request (see §5.2) |
| `transfer_out(dest: ServiceId, amount: Balance, memo: Vec<u8>)` | `()` | Transfer balance to another JAM service (AssetHub only) |
| `set_authorizer_queue(core: CoreIndex, queue: Vec<AuthorizerHash>, mode: QueueUpdateMode)` | `()` | Update the authorizer queue for a core (Coretime Chain only). `mode` determines whether the queue is applied immediately or cached in service state until the current 80-slot queue is exhausted. |
| `set_validator_keys(keys: Vec<ValidatorKey>)` | `()` | Set the next epoch's validator key set (AssetHub only) |
| `consume_transfers_up_to(index: u32)` | `()` | Mark all incoming transfers up to `index` as consumed. Accumulate prunes processed entries. When the queue is empty, index resets to 0. (AssetHub only) |
| `report_error(data: BoundedVec<u8, 1024>)` | `()` | Provide an opaque error payload (max 1024 bytes) before aborting the execution of the PVF. Stored per-parachain by Accumulate (see §3.3). |
| `register_parachain(para_id: ParaId, genesis_head: HeadData, validation_code_hash: ValidationCodeHash)` | `()` | Register a new parachain (Coretime chain only). See §6. |
| `deregister_parachain(para_id: ParaId)` | `()` | Deregister a parachain (Coretime chain only). See §6. |

Host functions that are tailored to a special parachain will lead to abortion if called by not authorized parachains.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage, roughly ~10ms of PVM gas per work result.

Accumulate for the Parachain Service performs the following operations, in order. They
cover the parachain-specific parts of what the relay chain's `enact_candidate` does today;
availability, approvals, disputes, and rewards are handled by JAM natively (see §2):

1. **Head data update + code upgrade check**: Writes the new `head_data` from the work result into `ParaInfo`
   for the parachain, records the relay-parent context, and immediately checks whether the
   candidate was validated with a pending new PVF code. If so, it activates the new code,
   calls `forget()` on the old code hash, and clears `pending_upgrade`. This must happen here
   because later candidates from the same parachain in the same block may already use the new code.
2. **Process host-function calls from Refine**: The work result carries the effects of host
   functions invoked by the PVF during Refine:
   - `request_code_upgrade` — calls `solicit(new_code_hash, code_len)` to request the
     preimage via the JAM preimage store, sets `pending_upgrade` with a deadline, and
     begins the dual-code transition period. See §5.2 for the full lifecycle.
   - `transfer_out` — calls `transfer()` to the destination JAM service.
   - `set_authorizer_queue` — either calls JAM `assign` immediately or caches the queue in
     `pending_authorizer_queues` for deferred application once the current 80-slot queue ends.
   - `set_validator_keys` — calls JAM `designate` to set upcoming validator keys.
3. **Incoming transfer processing**: Any `OnTransfer` calls received from other JAM services
   are appended to `incoming_transfers` **after all work reports in the block have been
   processed**. Asset Hub consumes them later via `consume_transfers_up_to(index)`.

The core Accumulate logic is primarily **parachain bookkeeping**: updating head data,
tracking code upgrades, applying queued authorizer updates, and managing incoming transfers.
Because selected work-reports are not replayed automatically, the service should checkpoint
after finishing each work-report so that progress survives any later out-of-gas or panic during
the same accumulation invocation.

### 5.2 Code Upgrade Lifecycle

Runtime (PVF) code upgrades follow a well-defined lifecycle using JAM's preimage
store (`solicit`/`provide`/`forget`) and the `xtpreimages` block extrinsic.

```
Phase 1: Request
  Parachain calls request_code_upgrade(new_code_hash) during Refine.
      │
      ▼
Phase 2: Request Preimage
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
  preimage store space with unused code. `UPGRADE_TIMEOUT` is set to **24 hours**, which
  should be sufficient for the preimage to be submitted to JAM after solicitation.
---

## 6. Parachain Registration

Parachain registration is managed by the **Coretime chain**, which owns the registration
policy (deposits, governance checks, quotas) and uses dedicated host functions to mutate
the Parachain Service's `parachains: Map<ParaId, ParaInfo>` state.

### 6.1 Registration Flow

```
Account on Coretime chain
    │  Submits registration with genesis head + validation code hash
    │  Places required deposit
    ▼
Coretime chain
    │  Governance / policy checks
    │  Calls register_parachain(para_id, genesis_head, validation_code_hash)
    ▼
Parachain Service (Accumulate)
    │  Inserts ParaInfo { head_data: genesis_head, validation_code_hash, pending_upgrade: None }
    │  Solicits the validation code preimage via the JAM preimage store
    │  Increments pvf_registry[validation_code_hash].ref_count
    ▼
User submits the validation code preimage to JAM (xtpreimages extrinsic)
    ▼
Parachain is live on its assigned core once the preimage is available
```

Registration does **not** wait for the preimage to be available — it only takes the
validation code hash. The Parachain Service solicits the preimage immediately, and the
user (typically the registering account) is responsible for submitting the actual PVF
bytecode via `xtpreimages` before any block can be validated.

The Coretime chain is responsible for:

- collecting and locking the registration deposit,
- validating governance policy (e.g. OpenGov approval, rate limiting, reserved IDs).

The Parachain Service is responsible for:

- inserting the initial `ParaInfo` entry,
- soliciting the validation code preimage,
- bumping the `pvf_registry` refcount for the validation code,
- rejecting duplicate `ParaId` registrations.

### 6.2 Deregistration Flow

```
Coretime chain
    │  Governance / policy approves removal (e.g. deposit reclaim, inactivity)
    │  Calls deregister_parachain(para_id)
    ▼
Parachain Service (Accumulate)
    │  Removes parachains[para_id]
    │  Decrements pvf_registry[validation_code_hash].ref_count
    │    → when refcount hits 0, forget() is called on the preimage
    │  Drops error_log[para_id], pending_upgrade, and any per-para state
```

Deposits and any economic unwinding are handled by the Coretime chain. Parachains
cannot deregister themselves — all lifecycle transitions go through the Coretime chain
to keep bookkeeping (deposits, quotas) in one place.

**Authorization:** both `register_parachain` and `deregister_parachain` are only callable
by the Coretime chain and abort when called by any other service. They are listed in the
main host-function table in §4.3.

---

## 7. Authorization & Coretime

JAM natively handles core assignment and coretime tracking, but the Parachain Service still needs
to decide **which authorizers are valid for each core**. For now, that control-plane function is
best modeled as an integration with the **Coretime Chain** (today's `pallet-broker` / coretime
system chain), which already owns the notion of who has rights over a core.

The important point is not the internals of authorizer execution, but the ownership boundary:

- The **Coretime Chain** decides which parachain or customer owns a core and therefore which
  authorizer queue should be installed for that core.
- The **Parachain Service** applies those decisions to JAM via its always-accumulate control path.
- JAM's `is_authorized` invocation then checks a work-package token against one of the authorizers
  currently in the core's authorizer pool.

This yields the following high-level flow:

```
Coretime Chain (pallet-broker)
    │  decides which parachain/customer controls core N
    │  and computes the desired authorizer queue for that core
    ▼
Parachain Service (always-accumulate control path)
    │  applies the queue update via JAM assign(core, queue, next_assigner)
    ▼
JAM authorizer pool / queue
    │  rotates authorizers from the queue into the pool over time
    ▼
Guarantors
    │  call is_authorized() before Refine
    ▼
JAM chain (Accumulate)
```

Two Gray Paper details matter here:

1. `assign` **overwrites the entire authorizer queue for a core immediately**.
2. The queue length is **80 entries** and the pool length is **8 entries**. Updating the queue is
   immediate, while actual authorizer eligibility changes gradually as the pool rotates entries in
   from the queue.

For the initial design we should therefore assume a simple model: the Coretime Chain (or a service
acting on its behalf) decides the full next 80-entry queue and the Parachain Service applies it
directly. If later we want delayed queue-rollover semantics, that should be specified explicitly as
service-level logic rather than assumed from JAM itself.

### 7.3 Authorizer Design: AURA Example

The authorizer is a single piece of PVM code (≤ 64 KB) deployed once as a preimage and
reused across all cores. Per-core behavior is controlled by the **config blob** (`pf`),
which is committed to when the authorizer queue is set via `assign`.

#### Config

The config encodes the parachain's collator set and slot timing:

```rust
struct AuthorizerConfig {
    /// Root of a binary Merkle trie over the collator public keys.
    /// Leaf index == collator index in the set.
    collator_set_root: Hash,
    /// Number of collators in the set.
    collator_set_size: u32,
    /// Slot duration as a multiple of the JAM timeslot (6s).
    /// E.g. slot_duration = 2 means one parachain slot every 12s.
    slot_duration: u32,
}
```

Since the config is hashed together with the authorizer code hash to form the authorizer
hash (`H(code_hash ⌢ config)`), the same authorizer hash is used for **every slot** in
the pool and queue as long as the collator set and slot duration remain unchanged.

When a parachain wants to **rotate its collator set** or **change its slot duration**, it
announces this to the Coretime Chain. The Coretime Chain then causes the Parachain Service's
always-accumulate path to apply a new authorizer queue for the relevant core, producing a new
authorizer hash from the same code and updated config.

#### Authorization Token

The collator includes an authorization token (`pj`) in the work package:

```rust
struct AuthorizationToken {
    /// Merkle proof that the collator's public key exists at the expected
    /// leaf index in the collator set trie.
    collator_proof: Vec<Hash>,
    /// The collator's public key.
    collator_key: PublicKey,
    /// Signature over the work package hash (excluding the token itself).
    signature: Signature,
}
```

#### Authorizer Logic

1. Decode config (`pf`) → `collator_set_root`, `collator_set_size`, `slot_duration`.
2. Decode token (`pj`) → `collator_proof`, `collator_key`, `signature`.
3. Read the **anchor timeslot** from the refinement context. If JAM does not yet expose it
   directly, this becomes a required host-interface extension (see §9).
4. Compute the expected collator index:
   `collator_index = (anchor_timeslot / slot_duration) mod collator_set_size`.
5. Verify `collator_proof` against `collator_set_root` at leaf `collator_index`,
   confirming `collator_key` is the expected collator for this slot.
6. Verify `signature` over the work package hash using `collator_key`.
7. Return a trace carrying the collator identity (or other minimal authorization metadata)
   needed by Refine/Accumulate.

#### Anchor Selection and Slot Claiming

The collator picks an anchor block (one of the last 8 JAM blocks) whose timeslot maps
to their collator index. Since the authorizer pool holds O = 8 entries — all with the
**same** authorizer hash — the collator can pick any of the 8 recent anchors.

This has a consequence: for **small collator sets** (< 8 collators), a collator could
claim **two consecutive blocks** by choosing different anchor blocks whose timeslots
both map to their index (e.g. with `collator_set_size = 4` and `slot_duration = 1`,
anchor timeslots T and T+4 both yield the same collator index).

This is acceptable — the Refine logic enforces that the **anchor timeslot is
non-decreasing** compared to the parachain's last included block. This prevents
a collator from going backwards, but allows the same collator to produce consecutive
blocks when the set is small. The non-decreasing constraint is not strict (equal is
allowed) to support **elastic scaling**, where multiple blocks for the same parachain
may reference the same anchor.

#### Collator Set Rotation Flow

```
Parachain runtime
    │  Decides to rotate collator set (e.g. via session change)
    │  Sends XCM to Coretime Chain with new collator set root + size
    ▼
Coretime Chain
    │  calls set_authorizer_queue(core, authorizers, mode)
    │  (new authorizer hashes computed from same code + updated config)
    ▼
Parachain Service (Accumulate)
    │  assign(core, new_queue)
    │  New authorizer hashes enter the pool via queue rotation
    ▼
Pool (8 entries)
    │  Old authorizer hashes drain out over ~8 blocks (48s)
    │  New ones rotate in
```

### 7.4 On-Demand Parachains

On-demand parachains (currently acquired via `pallet-on-demand`) continue to work: they
acquire a single-shot coretime allocation for one slot, submitted as a work package to the
Parachain Service for that slot only. This should remain **Coretime-Chain controlled** rather than
become an internal Parachain Service market. In other words, the Coretime Chain is responsible for
deciding who temporarily controls a core and for installing the corresponding authorizer queue.

One plausible extension is a fast-path market where an off-chain seller pre-registers authorizers
for short-lived slots and then resells access off-chain, but that is still conceptually a
Coretime-Chain policy question, not part of the Parachain Service itself.

---

## 8. Messaging

### 8.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, with a practical limit of ~1 MB per channel per block. UMP (Upward
Message Passing) similarly routes messages from parachain to relay chain.

### 8.2 Full XCMP on JAM

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
they are finalized.

For the Parachain Service:

- **Outgoing XCMP messages** from a parachain are included in the candidate commitments.
  During Refine, their payloads are exported to DA segments via `export()`. Accumulate
  records the payload hashes and updates channel metadata in the Parachain Service state.
- **Incoming XCMP messages** are fetched by the recipient parachain's collators directly
  from the DA layer by segment hash, keyed by the message hash recorded on-chain.

The exact host functions for HRMP channel management (open, accept, close) and XCMP message
handling are not yet specified. Additional host functions will likely be needed once the
messaging model is finalized.

---

## 9. Missing JAM / Gray Paper Features

The current design assumes two pieces of context that are not yet clearly exposed by the Gray Paper
host interface and therefore likely need either specification work or an explicit embedding into the
Parachain Service protocol:

1. **Anchor timeslot access**: the authorizer wants direct access to the anchor block's timeslot in
   order to derive the expected collator index. If this is not provided in the refinement context,
   JAM likely needs a dedicated host function or an equivalent context field.
2. **Lookup-anchor posterior state root access**: Parachain validation flows will need the
   posterior state root associated with the lookup anchor, not just its hash and timeslot. If that
   root is required, the refinement context or historical-lookup interface needs to expose it.

These are hard requirements, not optional refinements. The authorizer needs anchor-timeslot access
in order to derive the expected collator slot, and parachain validation is likely to need the
lookup-anchor posterior state root for state-proof reuse and retry scenarios where an earlier work
package failed to make it on-chain despite having a reusable PoV.

---

## 10. References

- [JAM Gray Paper](https://graypaper.com) — Formal JAM specification (Gavin Wood)
- [CoreJAM RFC #31](https://github.com/polkadot-fellows/RFCs/pull/31) — Original CoreJAM RFC
- [RFC-1: Agile Coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md)
- [RFC-5: Coretime Interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md)
- [Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
- [Polkadot Wiki: JAM Chain](https://wiki.polkadot.network/docs/learn-jam-chain)
- [Demystifying JAM](https://blog.kianenigma.com/posts/tech/demystifying-jam/) — Kian Paimani
- [JAM PVM Common API](https://docs.rs/jam-pvm-common/latest/jam_pvm_common/) — Host call specifications for Refine and Accumulate
- [JIP-1: Log Host Call](https://github.com/polkadot-fellows/JIPs/blob/main/JIP-1.md) — PVM logging specification
