# Parachain Service on JAM

> **Status**: Draft  
> **Last Updated**: 2026-04-22

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
6. [Parachain Management](#6-parachain-management)
   - 6.1 [Registration](#61-registration)
   - 6.2 [Forced Updates (Recovery)](#62-forced-updates-recovery)
   - 6.3 [Clean-up (Deregistration)](#63-clean-up-deregistration)
7. [Authorization & Coretime](#7-authorization-coretime)
   - 7.1 [Authorizer Design: AURA Example](#71-authorizer-design-aura-example)
   - 7.2 [On-Demand Parachains](#72-on-demand-parachains)
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
| Backing (validator group checks PoV) | **Guaranteeing** (Refine is one part) | Stateless off-chain validation + attestation |
| Availability | **Availability** | Confirm data is retrievable across the validator set |
| Approval | **Auditing** | Independent re-checks by other validators |
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

- **Refine (in-core)**: Executes `jam_validate_block` — the PVF validation that backing
  validators currently perform. Guarantors run the PVF against the PoV to verify the
  parachain block candidate. This replaces the current backing subsystem.
- **Accumulate (on-chain)**: Performs candidate enactment — updating head data, processing
  signals, managing channels and code upgrades. This replaces the current inclusion pallet
  logic.

```
┌────────────────────────────────────────────────────────────────────────────┐
│                                 JAM Chain                                  │
│                                                                            │
│  ON-CHAIN                                                                  │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                            Services Layer                            │  │
│  │                                                                      │  │
│  │  ┌────────────────────────────┐      ┌────────────────────────────┐  │  │
│  │  │     Parachain Service      │      │       Other Services       │  │  │
│  │  │                            │      │                            │  │  │
│  │  │     ┌────────────────┐     │      │     ┌────────────────┐     │  │  │
│  │  │     │  accumulate()  │     │      │     │  accumulate()  │     │  │  │
│  │  │     └────────────────┘     │      │     └────────────────┘     │  │  │
│  │  │                            │      │                            │  │  │
│  │  └────────────────────────────┘      └────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ········································································  │
│                                                                            │
│  IN-CORE                                                                   │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                          Parachain Service                           │  │
│  │                                                                      │  │
│  │                          ┌────────────────┐                          │  │
│  │                          │    refine()    │                          │  │
│  │                          └────────────────┘                          │  │
│  │                                                                      │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│  ▲                                                                         │
│  │ Guarantors execute Refine on assigned cores                             │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                  Data Availability (erasure-coded)                   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
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
              JAM validators attest (guarantee) its correctness. Availability of the work package is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate function runs on-chain.
              It records the new parachain head, applies the PVF's upward host-function
              effects (code upgrades, outbound transfers, authorizer updates, etc.), and queues
              incoming transfers from other services.

```

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

    /// Per-core timeslot at which `assign` was last called for that core.
    /// Combined with the 80-slot queue length, this lets the service compute
    /// when the current queue will be exhausted and a pending queue in
    /// `pending_authorizer_queues` should be applied.
    last_authorizer_assignment: Map<CoreIndex, Timeslot>,

    /// PVF (Parachain Validation Function) preimage registry.
    pvf_registry: Map<ValidationCodeHash, PvfEntry>,

    /// Stores when pending upgrades timeout.
    pending_upgrades_timeouts: Map<Timeslot, Vec<(ValidationCodeHash, ParaId)>>,
}

struct ErrorEntry {
    /// The error that caused this entry, either a structured Parachain
    /// Service error or the PVF's opaque payload.
    error: ParachainError,
    /// Authorizer trace from the work-report.
    auth_trace: Vec<u8>,
}

enum ParachainError {
    /// Refine used a validation code hash that matches neither
    /// `ParaInfo.validation_code_hash` nor the pending upgrade's code hash.
    /// Emitted by Accumulate when validating the work result.
    InvalidCodeHash,
    /// Opaque payload supplied by the PVF via `report_error(data)` before
    /// failing the execution (max 1024 bytes).
    Opaque(BoundedVec<u8, 1024>),
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

Each work package submitted to the Parachain Service contains one or more **work items**.
For the Parachain Service, a work item represents one parachain candidate. The candidate
itself — validation code hash and PoV — is carried as the work item's **extrinsic data**.

The shape of that extrinsic is:

```rust
struct ParachainCandidate {
    /// The hash of the currently active validation code. Used by Refine to
    /// look up the PVF bytecode from the preimage store.
    validation_code_hash: ValidationCodeHash,

    /// The Proof-of-Validity (PoV) — the actual block data + witness.
    pov: Vec<u8>,
}
```

Initially, each work package will contain a single work item (one parachain candidate).
Support for multiple items per package may be added later.

The `ParaId` for each work item is **not** stored in the work item itself. Instead, it is
sourced from the authorizer config, which is pinned by the Coretime chain (see §7.1). The
Parachain Service enforces that every authorizer config begins with a `Vec<ParaId>` whose
length matches the number of work items in the package, so that work item `item_index` is
authoritatively bound to `authorized_paras[item_index]`. Refine reads this prefix via
`auth_config()` and uses it to populate `ParachainWorkResult.para_id`.

### 3.3 Refine Result

The Parachain Service's Refine function returns a parachain work result per work item.
This result is forwarded to Accumulate. From the service's perspective, Refine either
succeeds or fails:

```rust
/// The Parachain Service's Refine output for one parachain candidate.
/// Side effects from host functions (code upgrades, transfers, authorizer
/// updates) are recorded separately during Refine and forwarded to Accumulate.
enum ParachainWorkResult {
    Ok {
        /// The parachain this result belongs to.
        para_id: ParaId,
        /// Hash of the validation code that Refine actually used.
        validation_code_hash: ValidationCodeHash,
        /// Hash of the parent head data this candidate was built on top of.
        parent_head_hash: Hash,
        /// New head data produced by the parachain block.
        head_data: HeadData,
        /// Upward messages emitted through host functions during Refine.
        /// Accumulate replays these in order.
        upward_messages: Vec<UpwardMessage>,
    },
    /// PVF execution failed (e.g. invalid PoV, bad state proof, panic).
    ///
    /// Carries a structured `ParachainError`. The PVF may call
    /// `report_error(data)` to provide an opaque payload (max 1024 bytes)
    /// before failing.
    Err {
        /// The parachain this failure belongs to.
        para_id: ParaId,
        error: ParachainError,
    },
}

enum UpwardMessage {
    /// From `request_code_upgrade` — start a PVF code upgrade (see §5.2).
    RequestCodeUpgrade(ValidationCodeHash),
    /// From `transfer_out` — transfer balance to another JAM service.
    TransferOut { dest: ServiceId, amount: Amount, memo: Memo },
    /// From `set_authorizer_queue` — update a core's authorizer queue.
    ///
    /// - `immediate`: when `true`, overwrite the queue immediately;
    ///   otherwise wait until the current queue is exhausted.
    /// - `new_assigner`: when `Some`, hands off `assigners[core]` to the
    ///   given service so it can manage its own queue from that point on;
    ///   when `None`, the current assigner is retained.
    SetAuthorizerQueue {
        core: CoreIndex,
        queue: Vec<AuthorizerHash>,
        immediate: bool,
        new_assigner: Option<ServiceId>,
    },
    /// From `set_validator_keys` — write the upcoming validator keys to
    /// JAM's `stagingset` via `designate`.
    SetValidatorKeys(Vec<ValidatorKey>),
    /// From `consume_transfers_up_to` — prune the consumed prefix of
    /// `incoming_transfers`.
    ConsumeTransfersUpTo(u32),
    /// From `parachain_set_head` — upsert a parachain's head data.
    ParachainSetHead { para_id: ParaId, new_head: HeadData },
    /// From `parachain_set_validation_code` — upsert a parachain's
    /// validation code hash.
    ParachainSetValidationCode { para_id: ParaId, new_validation_code_hash: ValidationCodeHash },
    /// From `parachain_clean_up` — remove all per-parachain state.
    ParachainCleanUp(ParaId),
}
```

The combined size of all result blobs plus the authorizer trace in a work-report is limited
to **48 KiB** by the Gray Paper.

- **`Ok`** is returned when PVF validation succeeds. The upward host-function calls made
  during Refine (code upgrades, transfers, authorizer updates, etc.) are carried alongside
  this result and applied by Accumulate.

- **`Err`** is returned when validation fails. It carries a `ParachainError`, which is
  either `InvalidCodeHash` (Accumulate detected the work result's `validation_code_hash`
  doesn't match any accepted code) or `Opaque(payload)` (the PVF called `report_error`
  before failing, up to 1024 bytes). Accumulate stores the error together with the
  authorizer trace in the per-parachain error log (see §3.1), keyed by the timeslot of the
  JAM block in which Accumulate is running. The entry is deleted when a later successful
  candidate for the same parachain is accumulated **whose lookup-anchor timeslot is
  strictly greater than the stored entry's key**. This can be used for example to slash a
  collator who claimed an authorizer slot that was not theirs.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

Refine is invoked **per work item** by JAM. For each work item at
index `item_index` the Parachain Service performs:

1. Reads the authorizer config via `auth_config()` and decodes the `authorized_paras`
   prefix; if `len(authorized_paras) != len(workitems)` this Refine invocation aborts
   with an `Err`.
2. Takes `para_id = authorized_paras[item_index]` as authoritative for this item.
3. Fetches the PVF bytecode via `historical_lookup` (using `validation_code_hash`).
4. Instantiates a child PVM with the PVF.
5. Executes the PVF against the PoV (the `jam_validate_block` call).
6. Assembles a `ParachainWorkResult` from the PVF's host-function side effects and the
   authoritative `para_id` (see §4.2).

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

The Refine wrapper also fails the invocation as `Err` if the PVF exits without calling
`set_parent_head_hash` exactly once — the parent-head declaration is mandatory.

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
| `set_parent_head_hash(hash: Hash)` | `()` | Declare the parent head hash this candidate was built on. **Mandatory**: every Refine invocation must call this exactly once or the invocation is invalid (treated as `Err`). The hash is forwarded to Accumulate. |
| `request_code_upgrade(hash: ValidationCodeHash)` | `()` | Signal a PVF code upgrade request (see §5.2) |
| `transfer_out(dest: ServiceId, amount: Balance, memo: Vec<u8>)` | `()` | Transfer balance to another JAM service (Asset Hub only) |
| `set_authorizer_queue(core: CoreIndex, queue: Vec<AuthorizerHash>, mode: QueueUpdateMode, new_assigner: Option<ServiceId>)` | `()` | Update the authorizer queue for a core (Coretime chain only). `mode` determines whether the queue is applied immediately or cached in service state until the current 80-slot queue is exhausted. `new_assigner`, when `Some`, hands off `assigners[core]` to another service so that service can manage its own core queue going forward; when `None`, the current assigner (Parachain Service) is retained. |
| `set_validator_keys(keys: Vec<ValidatorKey>)` | `()` | Write the upcoming validator keys to JAM's `stagingset` (Asset Hub only). Keys take effect two epoch transitions later, after flowing through `pendingset → activeset`. |
| `consume_transfers_up_to(index: u32)` | `()` | Mark all incoming transfers up to `index` as consumed. Accumulate prunes processed entries. When the queue is empty, index resets to 0. (Asset Hub only) |
| `report_error(data: BoundedVec<u8, 1024>)` | `()` | Provide an opaque error payload (max 1024 bytes) before aborting the execution of the PVF. Stored per-parachain by Accumulate (see §3.3). |
| `parachain_set_head(para_id: ParaId, new_head: HeadData)` | `()` | Upsert a parachain's head data (Coretime chain only). Used for both initial registration and recovery from a stuck chain. See §6. |
| `parachain_set_validation_code(para_id: ParaId, new_validation_code_hash: ValidationCodeHash)` | `()` | Upsert a parachain's validation code, bypassing the normal upgrade lifecycle (Coretime chain only). Used for both initial registration and forced code replacement. See §6. |
| `parachain_clean_up(para_id: ParaId)` | `()` | Remove all per-parachain state (Coretime chain only). See §6. |

Host functions that are restricted to specific services (e.g. Coretime chain, Asset Hub)
will abort with an error when called by any other service.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage.

Accumulate for the Parachain Service covers the parachain-specific parts of what the
relay chain's `enact_candidate` does today; availability, approvals, and disputes are handled
by JAM natively (see §2). The work splits into two categories:

#### Per-work-package work

Performed once for each work package that is being accumulated in this block, in order:

1. **Parent head check**: Verify the work result's `parent_head_hash` equals
   `hash(ParaInfo[para_id].head_data)`. If not, the candidate is rejected. This prevents
   a collator from including a candidate that was built on top of a stale, skipped, or
   non-canonical parent head.
2. **Validation code check**: Verify the work result's `validation_code_hash` matches
   either `ParaInfo.validation_code_hash` or the pending upgrade's code hash. If it
   matches neither, the candidate is rejected and an `ErrorEntry` with
   `ParachainError::InvalidCodeHash` is appended to `error_log[para_id]`.
3. **Head data update + code upgrade check**: Writes the new `head_data` from the work
   result into `ParaInfo` for the parachain and immediately checks whether the candidate
   was validated with the pending new PVF code. If so, it activates the new code, calls
   `forget()` on the old code hash, and clears `pending_upgrade`. This must happen here
   because later candidates from the same parachain in the same block may already use
   the new code. Any entries in `error_log[para_id]` whose key (timeslot) is strictly
   less than the current candidate's lookup-anchor timeslot are also pruned here.
4. **Process host-function calls from Refine**: Replay the `UpwardMessage`s carried in
   the work result, applying the effects of each side-effect host function the PVF
   invoked during Refine (code upgrades, transfers, authorizer queue updates, validator
   key updates, etc.). See the side-effect host function table in §4.3 for the full list.

#### General (always-accumulate) work

Performed once per block regardless of whether any parachain work packages were
accumulated, on the always-accumulate control path:

1. **Apply pending authorizer queues**: For each core whose current 80-slot queue has
   been exhausted (tracked via `last_authorizer_assignment`), drain the matching entry
   from `pending_authorizer_queues` and call JAM `assign` to install it.
2. **Reap timed-out code upgrades**: Walk `pending_upgrades_timeouts` for the current
   timeslot; for each entry still pending, call `forget()` on the new code hash and
   clear the parachain's `pending_upgrade`.
3. **Incoming transfer processing**: Append any `OnTransfer` calls received from other
   JAM services this block to `incoming_transfers`, **after all work reports in the
   block have been processed**. Asset Hub consumes them later via
   `consume_transfers_up_to(index)`.

The core Accumulate logic is primarily **parachain bookkeeping**: updating head data,
tracking code upgrades, applying queued authorizer updates, and managing incoming
transfers.
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

   > **Open question**: How exactly the dual-code cost is billed is not yet decided.
   > Upgrades are self-initiated by the parachain from within Refine via
   > `request_code_upgrade`, so the Coretime chain is not on the upgrade path and cannot
   > collect an upgrade-time deposit synchronously. Plausible options include: charging
   > the parachain's own service balance for the second preimage while `pending_upgrade`
   > is set (deducted by Accumulate, requires the service to carry balance); sizing the
   > base registration deposit to cover one concurrent pending upgrade (simple, but makes
   > registration more expensive for parachains that never upgrade); or pre-funding an
   > "upgrade credit" on the Coretime chain that the parachain draws down out-of-band
   > before attempting an upgrade. To be resolved alongside economic modeling of the
   > preimage store.
- **Permissionless submission**: The preimage can be submitted by anyone — the collator,
  block author, or any third party. The JAM protocol validates the hash against the
  solicitation.
- **Timeout protection**: The deadline prevents parachains from indefinitely occupying
  preimage store space with unused code. `UPGRADE_TIMEOUT` is set to **24 hours**, which
  should be sufficient for the preimage to be submitted to JAM after solicitation.

---

## 6. Parachain Management

Parachain lifecycle and management is driven by the **Coretime chain**, which owns the
policy layer: ParaId allocation, deposits, and deciding when to create, overwrite, or
clean up a parachain's state.

The Parachain Service itself deliberately exposes only three low-level, idempotent host
functions. Registration, forced updates, and deregistration all map onto them:

- `parachain_set_head(para_id, new_head)` — upsert head data
- `parachain_set_validation_code(para_id, new_validation_code_hash)` — upsert validation code
- `parachain_clean_up(para_id)` — remove all per-parachain state

All three are Coretime-chain-only; the Parachain Service performs no rights-checking of
its own and in particular **does not enforce ParaId uniqueness** — the Coretime chain is
the sole authority on which `ParaId`s are live and who owns them. If the Coretime chain
calls `parachain_set_head` / `parachain_set_validation_code` for an existing `ParaId`, the
service simply overwrites (useful for forced recovery). If it calls them for a fresh
`ParaId`, a new `ParaInfo` entry is created.

### 6.1 Registration

Registration is just the composition of `parachain_set_head` and
`parachain_set_validation_code` on a previously-unused `ParaId`:

```
Coretime chain
    │  Account submits registration with genesis head + validation code hash,
    │  placing the required deposit.
    │  Coretime chain allocates the ParaId and verifies it is not already live.
    │  Calls parachain_set_head(para_id, genesis_head)
    │  Calls parachain_set_validation_code(para_id, validation_code_hash)
    ▼
Parachain Service (Accumulate)
    │  Creates ParaInfo { head_data: genesis_head, validation_code_hash,
    │                     pending_upgrade: None }
    │  Solicits the validation code preimage via the JAM preimage store
    │  Increments pvf_registry[validation_code_hash].ref_count
    ▼
User submits the validation code preimage to JAM (xtpreimages extrinsic)
    ▼
Parachain is live on its assigned core once the preimage is available.
```

Registration does **not** wait for the preimage to be available — only the validation
code hash is needed. The Parachain Service solicits the preimage immediately, and then
anyone can submit the actual PVF bytecode via `xtpreimages`. Once the preimage is
available the parachain is ready to produce blocks.

### 6.2 Forced Updates (Recovery)

The same two host functions also handle exceptional recovery — e.g. unsticking a chain
whose last included block cannot be built on, or swapping in a new PVF outside the normal
upgrade lifecycle:

- `parachain_set_head(para_id, new_head)` overwrites `ParaInfo.head_data`.
- `parachain_set_validation_code(para_id, new_hash)` overwrites
  `ParaInfo.validation_code_hash`, solicits the new preimage, decrements the refcount of
  the old code (calling `forget()` if it drops to zero), and clears any `pending_upgrade`.

```
Coretime chain
    │  Verifies the rights of the caller
    │  Calls parachain_set_head(para_id, new_head) OR
    │        parachain_set_validation_code(para_id, new_validation_code_hash)
    ▼
Parachain Service (Accumulate)
    │  Applies the change, updates pvf_registry refcounts and preimage
    │  solicitations as needed. When the old validation code's refcount
    │  drops to zero, calls forget() on its preimage.
```

### 6.3 Clean-up (Deregistration)

```
Coretime chain
    │  Verifies the rights of the caller
    │  Calls parachain_clean_up(para_id)
    ▼
Parachain Service (Accumulate)
    │  Removes parachains[para_id]
    │  Decrements pvf_registry[validation_code_hash].ref_count
    │    → when refcount hits 0, forget() is called on the preimage
    │  Drops error_log[para_id], pending_upgrade, and any per-para state
```

Deposits and any economic unwinding are handled by the Coretime chain.

---

## 7. Authorization & Coretime

Coretime on JAM is managed by the **Coretime chain** for **all** services, not just the
Parachain Service. The Coretime chain decides which service — and, for the Parachain
Service, which parachain — owns each core and therefore which authorizer queue should be
installed on it. JAM itself tracks core assignment and coretime usage as protocol state.

For the Parachain Service, the ownership boundary is:

- The **Coretime chain** decides which parachain owns each core and computes the desired
  authorizer queue for that core.
- The **Parachain Service** applies those decisions to JAM via the JAM `assign` host call,
  emitted as an `UpwardMessage::SetAuthorizerQueue` from the PVF or from the
  always-accumulate control path.
- JAM's `is_authorized` invocation then checks a work-package token against one of the
  authorizers currently in the core's authorizer pool.

### 7.1 Authorizer Design: AURA Example

The authorizer is a single piece of PVM code (≤ 64 KB) deployed once as a preimage and
reused across all cores. Per-core behavior is controlled by the **config blob** (`pf`),
which is committed to when the authorizer queue is set via `assign`.

#### Config

The config encodes the parachain's collator set and slot timing:

```rust
struct AuthorizerConfig {
    /// Authoritative `ParaId` for each work item in the package, in the same
    /// order as `WorkPackage.workitems`.
    authorized_paras: Vec<ParaId>,
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
the pool and queue as long as the collator set, slot duration, and `authorized_paras`
remain unchanged.

When a parachain wants to **rotate its collator set** or **change its slot duration**, it
announces this to the Coretime chain.

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

1. Decode config (`pf`) → `authorized_paras`, `collator_set_root`, `collator_set_size`,
   `slot_duration`.
2. Decode token (`pj`) → `collator_proof`, `collator_key`, `signature`.
3. Read the **anchor timeslot** from the refinement context. If JAM does not yet expose it
   directly, this becomes a required host-interface extension (see §9).
4. Compute the expected collator index:
   `collator_index = (anchor_timeslot / slot_duration) mod collator_set_size`.
5. Verify `collator_proof` against `collator_set_root` at leaf `collator_index`,
   confirming `collator_key` is the expected collator for this slot.
6. Verify `signature` over the work package hash using `collator_key`.
7. Return a trace carrying the `collator_key`.

#### Parachain Service Enforcement

Independently of the authorizer code, the Parachain Service's **Refine wrapper** enforces:

- `Vec<ParaId>` (`authorized_paras`) is required to be the first bytes of the config blob.
- `len(authorized_paras) == len(workitems)` — rejects the package if they differ.

#### Anchor Selection and Slot Claiming

The collator picks an anchor block (one of the last 8 JAM blocks) whose timeslot maps
to their collator index. In steady-state AURA the authorizer queue is filled with the
**same** authorizer hash, so the pool's 8 entries are all the same hash and the collator
can pick any of the 8 recent anchors.

This has a consequence: for **small collator sets** (< 8 collators), a collator could
claim **two consecutive blocks** by choosing different anchor blocks whose timeslots
both map to their index (e.g. with `collator_set_size = 4` and `slot_duration = 6`,
anchor timeslots T and T+4 both yield the same collator index).

Preventing this is the responsibility of the **parachain's validation code**, not the
authorizer. If the PVF detects that the claimed anchor timeslot is inconsistent with the
parachain's own slot progression (e.g. the same collator claiming back-to-back slots they
are not entitled to), it can fail Refine and use `report_error(data)` to record a
structured complaint against the offending collator in the error log, which can then be
read by slashing logic of the parachain.

#### Collator Set Rotation Flow

```
Parachain runtime
    │  Decides to rotate collator set (e.g. via session change)
    │  Sends XCM to Coretime chain with new collator set root + size
    ▼
Coretime chain
    │  calls set_authorizer_queue(core, authorizers, mode, new_assigner)
    │  (new authorizer hashes computed from same code + updated config)
    ▼
Parachain Service (Accumulate)
    │  assign(core, new_queue, new_assigner)
    │  New authorizer hashes enter the pool via queue rotation
    ▼
Pool (up to 8 entries)
    │  Old authorizer hashes drain out over ~8 blocks (48s)
    │  New ones rotate in
```

### 7.2 On-Demand Parachains

On-demand coretime is not a special case for the Parachain Service — it is handled
entirely by the **Coretime chain**. When someone buys a single-slot coretime allocation,
the Coretime chain just calls `set_authorizer_queue(core, queue, immediate = true, ...)`
to install the buyer's authorizer on the target core for the duration of that slot. The
Parachain Service sees no difference between on-demand and bulk-purchased coretime; it
only sees a queue update.

Two plausible policies on the Coretime chain side:

- **Direct buyer authorization**: the authorizer for an on-demand slot simply verifies a
  signature from the buyer's key. The Coretime chain builds the authorizer config with
  the buyer's public key at the time of purchase.
- **Secondary market with pre-registered authorizers**: an off-chain service pre-registers
  generic authorizers on the Coretime chain and resells access tokens off-chain. Whoever
  holds a valid token can then submit work packages against the pre-registered authorizer.

In both cases, the Parachain Service implementation is unchanged — the Coretime chain
decides the policy, constructs the authorizer config, and calls `set_authorizer_queue`.

---

## 8. Messaging

### 8.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, and every byte is written into the relay-chain block. On
Polkadot mainnet the per-channel throughput is capped by the host configuration:

- `hrmpChannelMaxMessageSize` = **100 KiB** (per-message size cap)
- `hrmpChannelMaxTotalSize` = **100 KiB** (per-channel pending-bytes buffer)
- `hrmpChannelMaxCapacity` = **25** pending messages per channel
- `hrmpMaxMessageNumPerCandidate` = **10** HRMP messages per candidate
  (summed across all channels, not per channel)

So a parachain can emit at most 10 HRMP messages per block across all its channels, each
at most 100 KiB, and each channel can hold at most 100 KiB / 25 messages pending at a time.
UMP (Upward Message Passing) is similarly bounded — `maxUpwardMessageSize` ≈ 64 KiB and
`maxUpwardQueueSize` = 1 MiB on Polkadot mainnet.

On JAM, the buffer between Refine and Accumulate is even tighter: the work-report's
combined successful result blobs plus authorizer trace are bounded by **48 KiB**. All
upward messages the PVF emits through host functions have to fit inside that
budget alongside the new head data. Carrying HRMP-style message payloads through the
work-report is therefore not an option — they must go through a different channel, which
is what §8.2 proposes.

### 8.2 Proposed Solution: Full XCMP

The current HRMP model — routing full message payloads through the relay chain — cannot
work on JAM because the work result output is too small to carry message payloads on-chain.
Off-chain messaging is required.

The proposed model is **full XCMP**: only message *headers* and
*hashes* are recorded on-chain; the actual message payloads could be distributed off-chain via
JAM's data availability layer (D3L). This removes the per-message size bottleneck. The
Refine function uses `export()` to write outbound message payloads into DA segments, and
Accumulate only records the message hashes and channel metadata on-chain. See
[paritytech/polkadot-sdk#10449](https://github.com/paritytech/polkadot-sdk/pull/10449)
for a potential specification of XCMP.

The exact host functions for HRMP channel management (open, accept, close) and XCMP message
handling are not yet specified. Additional host functions will likely be needed once the
messaging model is finalized.

---

## 9. Missing JAM / Gray Paper Features

The current design assumes two pieces of context that are not yet clearly exposed by the Gray Paper
host interface and therefore likely need either specification work or an explicit embedding into the
Parachain Service protocol:

1. **Anchor timeslot access**: the authorizer needs direct access to the anchor block's
   timeslot in order to derive the expected collator index. If this is not provided in the
   refinement context, JAM likely needs a dedicated host function or an equivalent context
   field.
2. **Lookup-anchor posterior state root access**: parachain validation flows will need the
   posterior state root associated with the lookup anchor, not just its hash and timeslot,
   to support state-proof reuse and retry scenarios where an earlier work package failed
   to make it on-chain despite having a reusable PoV.

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
