# Parachain Service on JAM

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [The Parachain Service](#3-the-parachain-service)
   - 3.1 [Service State Layout](#31-service-state-layout)
   - 3.2 [Work Items](#32-work-items)
   - 3.3 [Work Digest](#33-work-digest)
4. [Refine: In-Core Execution](#4-refine-in-core-execution)
   - 4.1 [What Refine Does](#41-what-refine-does)
   - 4.2 [PVF Entry Point](#42-pvf-entry-point)
   - 4.3 [Host Functions & PVM Imports](#43-host-functions-pvm-imports)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
   - 5.1 [What Accumulate Does](#51-what-accumulate-does)
   - 5.2 [Code Upgrade Lifecycle](#52-code-upgrade-lifecycle)
   - 5.3 [Validator-Key Updates](#53-validator-key-updates)
   - 5.4 [Service Self-Upgrade](#54-service-self-upgrade)
6. [Parachain Management](#6-parachain-management)
   - 6.1 [State-Balance Accounting](#61-state-balance-accounting)
   - 6.2 [Registration](#62-registration)
   - 6.3 [Forced Updates (Recovery)](#63-forced-updates-recovery)
   - 6.4 [Clean-up (Deregistration)](#64-clean-up-deregistration)
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

    /// Incoming transfer queue for Asset Hub.
    /// See §5.1.
    incoming_transfers: BoundedVec<(ServiceId, Amount, Memo), 1000>,

    /// Per-parachain log, keyed only by ParaId. Each entry carries its
    /// Timeslot inline; multiple entries may share a timeslot (e.g. several
    /// work packages at the same height each producing a refine error). The
    /// log's total encoded size is bounded to 64 KiB; entries are evicted and
    /// pruned during Accumulate (see §5.1).
    parachain_log: Map<ParaId, Vec<(Timeslot, LogEntry)>>,

    /// Pending authorizer queue updates, keyed by core.
    pending_authorizer_queues: Map<CoreIndex, BoundedVec<AuthorizerHash, 80>>,

    /// Dirty-core index: each core with a pending queue, paired with the
    /// `jam_slot` at which the update becomes due. Lets the always-accumulate
    /// path find and gate due updates without scanning all cores (see §5.1).
    pending_authorizer_cores: BoundedVec<(CoreIndex, Timeslot), CoreCount>,

    /// Cross-parachain preimage registry. Holds every preimage the service
    /// has solicited from JAM — including each parachain's active validation
    /// code, any pending-upgrade code, and PVF-initiated `solicit` requests —
    /// under the same referencer-multiplexing scheme. In the key, `Hash` is
    /// the preimage's hash and `u32` its byte length. See §6.1.
    preimage_registry: Map<(Hash, u32), PreimageEntry>,

    /// Validator-key set being assembled chunk by chunk by
    /// `set_validator_keys`. See §5.3.
    staged_validator_keys: BoundedVec<ValidatorKey, 1023>,

    /// Per-parachain key/value store. 
    ///
    /// See §6.1 for the per-entry formula.
    key_value_storage: Map<(ParaId, Vec<u8>), Vec<u8>>,
}

enum LogEntry {
    Refine(RefineLogEntry),
    Accumulate(AccumulateLogEntry),
}

struct RefineLogEntry {
    /// What went wrong during Refine.
    error: RefineLog,
    /// Authorizer trace from the work-report that produced this failure,
    /// truncated to 256 bytes.
    auth_trace: BoundedVec<u8, 256>,
}

struct AccumulateLogEntry {
    /// Events recorded while Accumulating a work digest for this parachain.
    /// Not separately count-bounded; the whole `parachain_log` is held within
    /// its 64 KiB byte cap by eviction during Accumulate (see §5.1).
    entries: Vec<AccumulateLog>,
}

enum RefineLog {
    /// `historical_lookup(validation_code_hash)` returned `None`: the
    /// validation code preimage is not available in the service's store
    /// at the lookup-anchor. See §4.1 step 4.
    InvalidCodeHash,
    /// Opaque payload supplied by the PVF via `report_error(data)` before
    /// failing the execution (max 1024 bytes).
    Opaque(BoundedVec<u8, 1024>),
    /// A `set_validator_keys` chunk contained more than 30 keys, or
    /// `set_validator_keys` was called more than once in a single Refine
    /// invocation. See §4.3, §5.3.
    SetValidatorKeysTooManyKeys,
    /// The PVF emitted more than 1024 upward messages in a single Refine
    /// invocation. See §4.3.
    TooManyUpwardMessages,
    /// The PVF invoked a host function restricted to another parachain
    /// (Asset Hub or the Coretime chain). See §4.3.
    RestrictedHostFunction,
    /// The authorizer config's `authorized_paras` prefix length does not
    /// match the work package's item count. See §4.1 step 1.
    AuthConfigMismatch,
    /// The work package has 0 items or more than 1 item; only single-item
    /// packages are currently supported (§3.2).
    InvalidItemCount,
    /// The opaque `AuthorizerConfig` blob failed to decode. See §4.1 step 1.
    MalformedAuthorizerConfig,
    /// The work item payload failed to decode into a `ParachainCandidate`.
    /// See §4.1 step 3.
    MalformedPayload,
    /// The encoded `ParachainWorkDigest` (head data + upward messages) would
    /// exceed the Gray Paper's 48 KiB combined result-blob + auth-trace
    /// budget. See §4.1.
    WorkDigestTooLarge,
    /// The PVF exited without calling `set_parent_head_hash` and/or `set_head`
    /// exactly once. Both head declarations are mandatory. See §4.2.
    MissingHeadDeclaration,
}

/// Why a state-balance reservation failed (see §6.1).
enum InsufficientBalanceReason {
    /// A `solicit` (or code-upgrade solicit) of the preimage with `hash` and `len`.
    Solicit { hash: Hash, len: u32 },
    /// A `kv_set(key, value)` write to `key_value_storage`. Only the
    /// blake2-256 hash of `key` is recorded so an arbitrarily large
    /// user key cannot inflate `parachain_log`.
    SetKV { key_hash: Hash },
}

enum AccumulateLog {
    /// The work digest's `validation_code_hash` matches neither the active
    /// `ParaInfo.validation_code` nor the pending upgrade's code hash.
    /// This is the authoritative validation-code check (Refine does not
    /// perform it). See §5.1 step 3.
    InvalidCodeHash { hash: ValidationCodeHash },
    /// Available state balance insufficient for the operation described by
    /// `reason`. See §6.1.
    InsufficientStateBalance { reason: InsufficientBalanceReason },
    /// `parachain_set_state_balance(para_id, attempted)` was rejected
    /// because `attempted < current_used`. See §6.1.
    StateBalanceUpdateRejected {
        attempted: Compact<Balance>,
        current_total: Compact<Balance>,
        current_used: Compact<Balance>,
    },
    /// JAM `designate` rejected the assembled validator-key set because its
    /// `len` is not in `valcount`. The staging buffer is cleared regardless. See §5.3.
    DesignateRejected { len: u32 },
    /// A `set_validator_keys` chunk would grow `staged_validator_keys` beyond
    /// its reserved capacity (`MaxStagedValidatorKeys`); the append is rejected
    /// and the buffer left unchanged. See §5.3.
    StagedValidatorKeysOverflow,
    /// `parachain_service_upgrade(code_hash, ...)` was rejected because
    /// `code_hash`'s preimage is not in the Parachain Service's preimage
    /// store. See §5.4.
    ServiceUpgradePreimageMissing { code_hash: Hash },
    /// The JAM `transfer` call replaying a `TransferOut`. Only the memo
    /// hash is recorded — the full 128-byte memo is not preserved. See §5.1 step 7.
    TransferFailed { memo_hash: Hash },
    /// A `forget` removed the last referencer without expunging the preimage;
    /// the caller must `forget(hash, len)` again *after* `due` (strictly, once
    /// the current timeslot exceeds `due`) to free it. See §6.1.
    ForgetAgainAt { hash: Hash, len: u32, due: Timeslot },
    /// `parachain_clean_up` was rejected because the parachain still holds state
    /// beyond its baseline and validation code(s); it must release the rest
    /// first. See §6.4.
    TooMuchStateHeld,
}

struct PreimageEntry {
    /// Parachains currently referencing this preimage. Bounded by the
    /// protocol-level maximum number of parachains.
    referencers: BoundedBTreeSet<ParaId>,
}

/// Head data is capped at 4 KiB to bound the per-parachain footprint that
/// `ParaInfo` contributes to the baseline state-balance reservation (see §6.1).
type HeadData = BoundedVec<u8, { 4 * 1024 }>;

/// Fixed 128-byte transfer memo, matching Gray Paper `C_memosize = 128`.
type Memo = [u8; 128];

/// A validation code reference: its hash plus its SCALE-encoded byte length.
struct ValidationCodeRef {
    hash: ValidationCodeHash,
    len: u32,
}

/// A validation code with its reference and `pinned` flag — whether the
/// parachain has *also* solicited it itself, on top of the service's own
/// code-upgrade solicit. See §5.2.
struct ValidationCode {
    ref: ValidationCodeRef,
    pinned: bool,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Currently active validation code, or `None` for a freshly-registered
    /// parachain. See §6.
    validation_code: Option<ValidationCode>,
    /// Pending code upgrade, if any: the new validation code and the
    /// deadline timeslot after which the upgrade is rejected. See §5.2.
    pending_upgrade: Option<(ValidationCode, Timeslot)>,
    /// Total state balance allocated to this parachain. Set exclusively by
    /// the Coretime chain via `parachain_set_state_balance`. See §6.1.
    total_state_balance: Compact<Balance>,
    /// State balance currently consumed by this parachain's solicited PVF
    /// preimages (active validation code + pending upgrade, if any).
    /// Increased on `solicit()`, decreased on `forget()`. See §6.1.
    used_state_balance: Compact<Balance>,
    /// Set once `parachain_clean_up` has begun deregistering this parachain
    /// but some preimage still awaits its second, expunging `forget`. See §6.4.
    is_deregistering: bool,
}
```

#### Storage key encoding

Each storage item — a top-level `Map` or a singleton — is assigned a distinct
**1-byte tag** identifying it within the service's JAM storage. The full JAM
storage key is `[tag: u8] || SCALE-encoded logical key` (the tag alone for
singletons; the tag prepended to the encoded map key for map entries). 

| Tag | Storage item |
|--------|------------------------------|
| `0x00` | `parachains` |
| `0x01` | `parachain_log` |
| `0x02` | `pending_authorizer_queues` |
| `0x03` | `pending_authorizer_cores` |
| `0x04` | `preimage_registry` |
| `0x05` | `staged_validator_keys` |
| `0x06` | `incoming_transfers` |
| `0x07` | `key_value_storage` |

### 3.2 Work Items

Each work package submitted to the Parachain Service contains one or more **work items**.
For the Parachain Service, a work item represents one parachain candidate. The candidate
itself — validation code hash and PoV — is carried entirely in the work item's
**payload** as a single SCALE-encoded blob.

The shape of that payload is:

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
`auth_config()` and uses it to populate `ParachainWorkDigest.para_id`.

### 3.3 Work Digest

The Parachain Service's Refine function returns a parachain work digest per work item.
This digest is forwarded to Accumulate. From the service's perspective, Refine either
succeeds or fails:

```rust
/// The Parachain Service's Refine output for one parachain candidate.
/// Side effects from host functions (code upgrades, transfers, authorizer
/// updates) are recorded separately during Refine and forwarded to Accumulate.
enum ParachainWorkDigest {
    Ok {
        /// The parachain this digest belongs to.
        para_id: ParaId,
        /// The validation code that Refine actually used to check the candidate.
        validation_code: ValidationCodeRef,
        /// Hash of the parent head data this candidate was built on top of.
        parent_head_hash: Hash,
        /// New head data produced by the parachain block.
        head_data: HeadData,
        /// Upward messages emitted through host functions during Refine.
        /// Accumulate replays these in order.
        upward_messages: Vec<UpwardMessage>,
        /// The work package's lookup-anchor timeslot.
        lookup_anchor: Timeslot,
    },
    /// PVF execution failed (e.g. invalid PoV, bad state proof, panic).
    ///
    /// Carries a structured `RefineLog`. The PVF may call
    /// `report_error(data)` to provide an opaque payload (max 1024 bytes)
    /// before failing.
    Err {
        /// The parachain this failure belongs to.
        para_id: ParaId,
        error: RefineLog,
    },
}

enum UpwardMessage {
    /// From `request_code_upgrade` — start a PVF code upgrade (see §5.2).
    RequestCodeUpgrade { hash: ValidationCodeHash, len: u32 },
    /// From `solicit` — request a preimage be made available in the
    /// parachain's own preimage store. Accumulate forwards this to JAM
    /// and increments `ParaInfo.used_state_balance` (rejected if it
    /// would exceed `total_state_balance`). See §6.1.
    Solicit { hash: Hash, len: u32 },
    /// From `forget` — release a previously solicited preimage. Removing the
    /// last referencer may need a follow-up `forget` (two-step expunge; see
    /// §6.1). For the parachain's active or pending validation code it only
    /// clears `pinned` (§5.2).
    Forget { hash: Hash, len: u32 },
    /// From `kv_set` — upsert `key_value_storage[(para_id, key)] = value`.
    /// Accumulate replays it with delta state-balance charging (see §6.1).
    SetKV { key: Vec<u8>, value: Vec<u8> },
    /// From `kv_remove` — remove `key_value_storage[(para_id, key)]`, refunding
    /// its footprint (see §6.1).
    RemoveKV { key: Vec<u8> },
    /// From `transfer_out` — transfer balance to another JAM service.
    TransferOut { dest: ServiceId, amount: Compact<Amount>, memo: Memo },
    /// From `set_authorizer_queue` — schedule a core's authorizer QUEUE.
    ///
    /// - `jam_slot`: the JAM timeslot at which the queue is applied. The queue is
    ///   cached and forwarded to JAM in the always-accumulate phase once the
    ///   timeslot reaches `jam_slot` (§5.1); if `jam_slot` is already due
    ///   (`jam_slot <= now`) when the message is processed, the queue is applied
    ///   inline right away.
    /// - `queue`: an empty list cancels any queue cached for the core so it is
    ///   not applied.
    SetAuthorizerQueue {
        core: CoreIndex,
        queue: Vec<AuthorizerHash>,
        jam_slot: Timeslot,
    },
    /// From `set_assigner` — change a core's assigner (`assigners[core]`).
    /// `Some(s)` hands it off to service `s` so it can manage the core's queue
    /// from that point on; `None` resets the assigner.
    SetAssigner {
        core: CoreIndex,
        new_assigner: Option<ServiceId>,
    },
    /// From `set_validator_keys`. See §5.3.
    SetValidatorKeys { keys: Vec<ValidatorKey>, is_last: bool },
    /// From `consume_transfers_up_to` — prune the consumed prefix of
    /// `incoming_transfers`.
    ConsumeTransfersUpTo(u32),
    /// From `parachain_service_upgrade`. See §5.4.
    UpgradeService { code_hash: Hash, len: u32, min_item_gas: u64, min_memo_gas: u64 },
    /// From `parachain_set_head` — upsert a parachain's head data.
    ParachainSetHead { para_id: ParaId, new_head: HeadData },
    /// From `parachain_set_validation_code` — upsert a parachain's
    /// validation code hash. The service must solicit the validation
    /// code preimage.
    ParachainSetValidationCode {
        para_id: ParaId,
        new_validation_code_hash: ValidationCodeHash,
        new_validation_code_len: u32,
    },
    /// From `parachain_clean_up` — remove all per-parachain state.
    ParachainCleanUp(ParaId),
    /// From `parachain_set_state_balance`. See §6.1.
    ParachainSetStateBalance { para_id: ParaId, new_total: Compact<Balance> },
}
```

The combined size of all result blobs plus the authorizer trace in a work-report is limited
to **48 KiB** by the Gray Paper.

- **`Ok`** is returned when PVF validation succeeds. The upward host-function calls made
  during Refine (code upgrades, transfers, authorizer updates, etc.) are carried alongside
  this digest and applied by Accumulate.

- **`Err`** is returned when Refine fails (see `RefineLog`). Accumulate appends
  a `LogEntry::Refine` to the parachain's `parachain_log` (see §3.1) together
  with the work-report's authorizer trace — useful for example to slash a
  collator who claimed an authorizer slot that was not theirs.

> **JAM `WorkErrorCode` is skipped.** When JAM substitutes a work-item with
> a gray paper `WorkExecResult::Error(WorkErrorCode)`, the Parachain
> Service's refine wrapper never produces a `ParachainWorkDigest`. The
> service does not progress that work-item: Accumulate skips it as if it
> did not exist — no `parachain_log` entry, no state change.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

Refine is invoked **per work item** by JAM. For each work item at
index `item_index` the Parachain Service performs:

1. Reads the authorizer config via `auth_config()` and decodes the `authorized_paras`
   prefix; if `len(authorized_paras) != len(workitems)` this Refine invocation aborts
   with an `Err`.
2. Takes `para_id = authorized_paras[item_index]` as authoritative for this item.
3. Decodes the `ParachainCandidate` (validation code hash + PoV) from the work item
   payload passed to Refine. If the payload fails to decode, aborts with
   `Err(RefineLog::MalformedPayload)`.
4. Fetches the PVF bytecode via `historical_lookup` (using `validation_code_hash`).
   If the lookup returns `None` (the preimage isn't available in the service's
   store at the lookup-anchor), aborts with `Err(RefineLog::InvalidCodeHash)`.
5. Instantiates a child PVM with the PVF.
6. Executes the PVF against the PoV (the `jam_validate_block` call).
7. Assembles a `ParachainWorkDigest` from the PVF's host-function side effects and the
   authoritative `para_id` (see §4.2).
8. Checks that the encoded digest (head data + upward messages) plus the
   work-report's authorizer trace fits in the Gray Paper's 48 KiB
   combined-result-blob budget; if not, aborts with
   `Err(RefineLog::WorkDigestTooLarge)`.

Because Refine is stateless, it cannot write to service storage.

### 4.2 PVF Entry Point

The Parachain Service's Refine spawns a child PVM and calls the PVF's single entry point:

```rust
fn jam_validate_block() -> ()
```

The PVF reads its inputs (PoV, context, downward transfers) through host functions and
writes its outputs (head data, code upgrades, transfers) through host functions. It does
not return a value directly — the `ParachainWorkDigest` is assembled by the Parachain
Service's Refine wrapper from the accumulated host-function side effects.

If the PVF exits abnormally (panic, trap, or other failed execution), Refine treats this as
`Err` and records the opaque error payload previously supplied through `report_error(data)` if
one was provided.

The Refine wrapper also fails the invocation as `Err` if the PVF exits without calling
`set_parent_head_hash` exactly once or without calling `set_head` exactly once — both the
parent-head and the new-head declarations are mandatory.

### 4.3 Host Functions & PVM Imports

On JAM, PVFs execute inside a child PVM instance spawned by the Parachain Service's Refine
function. **Hashing**, and **signature verification** are expected to
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
| `work_package_context()` | `RefineContext` | Access the refinement context: anchor (hash, timeslot, posterior state-root), lookup-anchor (hash, timeslot, posterior state-root), prerequisites |
| `auth_config()` | `Vec<u8>` | Access the authorizer config blob |
| `auth_token()` | `Vec<u8>` | Access the authorization token blob |
| `work_items_summary()` | `Vec<WorkItemSummary>` | Summary of all work items (service, code hash, gas limits, counts, payload length) |
| `work_item_summary(index: u32)` | `Option<WorkItemSummary>` | Summary of a specific work item by index |
| `work_item_payload(index: u32)` | `Option<Vec<u8>>` | Payload of a specific work item by index |
| `import_segments()` | `Vec<SegmentMeta>` | Import segments metadata |
| `import_segment(index: u32)` | `Option<Vec<u8>>` | A specific import segment by index |

#### Side-effect host functions

These produce effects carried in the work digest and applied by Accumulate:

| Host function | Returns | Purpose |
|---|---|---|
| `export(data: Vec<u8>)` | `u32` | Write a segment to the JAM Data Lake (e.g. outbound XCMP payloads). Returns segment index. |
| `set_parent_head_hash(hash: Hash)` | `()` | Declare the parent head hash this candidate was built on. **Mandatory**: every Refine invocation must call this exactly once or the invocation is invalid (treated as `Err`). The hash is forwarded to Accumulate. |
| `set_head(new_head: HeadData)` | `()` | Declare the new head data this parachain block produced. **Mandatory**: every Refine invocation must call this exactly once or the invocation is invalid (treated as `Err`). The head data is forwarded to Accumulate as `ParachainWorkDigest.head_data` and written into `ParaInfo.head_data` on enactment (§5.1 step 5). Distinct from the Coretime-only `parachain_set_head`, which forcibly overwrites *another* para's head outside the normal block lifecycle (§6). |
| `request_code_upgrade(hash: ValidationCodeHash, len: u32)` | `()` | Signal a PVF code upgrade request. See §5.2. |
| `solicit(hash: Hash, len: u32)` | `()` | Mediated forward of JAM's `solicit` (see §6.1). Idempotent — no-op if the parachain is already in `preimage_registry[hash].referencers`. May fail with `InsufficientStateBalance`. For the parachain's own active/pending validation code it only sets `pinned` to true (§5.2). |
| `forget(hash: Hash, len: u32)` | `()` | Mediated forward of JAM's `forget` (see §6.1). Idempotent — no-op if the parachain is not in `preimage_registry[hash].referencers`. May be called for the parachain's own active/pending validation code, where it only sets `pinned` to false (§5.2). |
| `kv_set(key: Vec<u8>, value: Vec<u8>)` | `()` | Upsert `key_value_storage[(para_id, key)] = value`, delta-charged against `used_state_balance` (see §6.1). May fail with `InsufficientStateBalance` when a size increase would exceed `total_state_balance`. |
| `kv_remove(key: Vec<u8>)` | `()` | Remove `key_value_storage[(para_id, key)]`, refunding its footprint (see §6.1). Idempotent — no-op if the key is absent. |
| `transfer_out(dest: ServiceId, amount: Balance, memo: Memo)` | `()` | Transfer balance to another JAM service (Asset Hub only). If the JAM `transfer` call fails during `accumulate`, an `AccumulateLog::TransferFailed { memo_hash }` entry is appended to the parachain's log. See §5.1 step 7. |
| `set_authorizer_queue(core: CoreIndex, queue: Vec<AuthorizerHash>, jam_slot: Timeslot)` | `()` | Schedule the authorizer queue for a core (Coretime chain only). The queue is cached in service state and forwarded to JAM in the always-accumulate phase once the timeslot reaches `jam_slot`; if `jam_slot` is already due (`jam_slot <= now`) when the call is processed, it is applied inline right away. An empty `queue` cancels any queue cached for the core so it is not applied. |
| `set_assigner(core: CoreIndex, new_assigner: Option<ServiceId>)` | `()` | Change `assigners[core]` (Coretime chain only): `Some(s)` hands it off to service `s` so it can manage that core's queue going forward; `None` resets the assigner. |
| `set_validator_keys(keys: Vec<ValidatorKey>, is_last: bool)` | `()` | Append a chunk of upcoming validator keys to `staged_validator_keys` (Asset Hub only); see §5.3. Panics if `keys` contains more than **30 keys** or if called more than once per Refine invocation. |
| `consume_transfers_up_to(index: u32)` | `()` | Mark all incoming transfers up to `index` as consumed; Accumulate prunes those entries. (Asset Hub only) |
| `parachain_service_upgrade(code_hash: Hash, len: u32, min_item_gas: u64, min_memo_gas: u64)` | `()` | Replace the Parachain Service's own service code by forwarding JAM `upgrade(code_hash, min_item_gas, min_memo_gas)` (Asset Hub only). `len` is the new code's SCALE-encoded byte length. Accumulate rejects the call with `AccumulateLog::ServiceUpgradePreimageMissing` if the new code's preimage is not in the Parachain Service's preimage store. See §5.4. |
| `report_error(data: BoundedVec<u8, 1024>)` | `()` | Provide an opaque error payload before aborting the execution of the PVF; any bytes beyond 1024 are truncated. Stored per-parachain by Accumulate (see §3.3). |
| `parachain_set_head(para_id: ParaId, new_head: HeadData)` | `()` | Upsert a parachain's head data (Coretime chain only). Used for both initial registration and recovery from a stuck chain. See §6. |
| `parachain_set_validation_code(para_id: ParaId, new_validation_code_hash: ValidationCodeHash, new_validation_code_len: u32)` | `()` | Upsert a parachain's validation code, bypassing the normal upgrade lifecycle (Coretime chain only). `new_validation_code_len` is the byte length of the PVF preimage, used by the service to solicit the preimage from JAM and to charge the para's `used_state_balance` for it. Used for both initial registration and forced code replacement. See §6. |
| `parachain_clean_up(para_id: ParaId)` | `()` | Remove all per-parachain state (Coretime chain only). See §6. |
| `parachain_set_state_balance(para_id: ParaId, new_total: Balance)` | `()` | Overwrite `ParaInfo[para_id].total_state_balance` (Coretime chain only). On rejection an `AccumulateLog::StateBalanceUpdateRejected` is appended to the parachain's log. See §6.1. |

Host functions that are restricted to specific parachains (e.g. Coretime chain, Asset Hub)
will abort with an error when called by any other parachain.

A single Refine invocation may emit at most **1024 upward messages**. If the PVF
exceeds this, the invocation fails with `Err(RefineLog::TooManyUpwardMessages)`.
This bounds the number of side effects Accumulate must replay per work item,
independently of the 48 KiB combined-result-blob budget.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage.

Accumulate for the Parachain Service covers the parachain-specific parts of what the
relay chain's `enact_candidate` does today; availability, approvals, and disputes are handled
by JAM natively (see §2). The work runs in three phases, in order: all always-accumulate
work first — due authorizer-queue flushes, then incoming-transfer processing — and then
per-work-package work. Because always-accumulate runs *before* the work packages, a queue
a work package schedules this block is normally not flushed in the same block: it is applied
by a later block's always-accumulate once its `jam_slot` arrives. The exception is a queue
whose `jam_slot` is already due (`jam_slot <= now`) when the scheduling message is processed —
since always-accumulate has already run, it is applied inline right away and any scheduled queue
update for the core is removed.

#### Apply due authorizer queues (before work packages)

Iterate `pending_authorizer_cores` and, for each `(core, jam_slot)` pair, check whether
the entry is due: `now >= jam_slot`, read directly from the pair without touching
`pending_authorizer_queues`. If due, drain the entry from `pending_authorizer_queues`,
remove the pair from `pending_authorizer_cores`, and call JAM `assign` to install it.

#### Incoming transfer processing (before work packages)

Append any `OnTransfer` calls received from other JAM services this block to
`incoming_transfers`. If the queue is full the service bounces the funds back to the
sender with the same memo.

A transfer whose memo is all 1-bits (`[0xFF; 128]`) is a **recovery transfer**: it is
neither queued nor bounced. Its balance still credits the Parachain Service, so it is a
way to fund the Parachain Service when it has run out of storage and can therefore no
longer grow `incoming_transfers` to accept (or bounce) normal transfers.

#### Per-work-package work

Performed once for each work package that is being accumulated in this block, in order.
A work result of gray-paper `WorkExecResult::Error` indicates a bug in the parachain
service's `refine` and is skipped entirely here — no `parachain_log` entry, no state
change, and it never reaches the steps below. Otherwise:

1. **Registration check**: Reject the work-package immediately, and record no
   `parachain_log` entry, if `para_id` is not in `parachains` or its `ParaInfo`
   has `is_deregistering == true` (§6.4) — a deregistering para is treated as if
   it no longer exists.
 2. **Refine-result dispatch**: If the work digest is a **Refine failure**
    (`ParachainWorkDigest::Err`, where `refine` completed and returned an error digest —
    see §3.3), forward its `RefineLog` into a `RefineLogEntry` appended to
    `parachain_log[para_id]` (the work-report's authorizer trace is already attached)
    under the eviction rules below, then stop — no further steps run and no log
    pruning is done. A **Refine success**
    (`ParachainWorkDigest::Ok`) proceeds through the remaining steps.
3. **Parent head check**: Verify the work digest's `parent_head_hash` equals
   `hash(ParaInfo[para_id].head_data)`. If not, the candidate is rejected. This prevents
   a collator from including a candidate that was built on top of a stale, skipped, or
   non-canonical parent head.
4. **Reap timed-out pending upgrade**: If `ParaInfo.pending_upgrade` is set
   and its deadline timeslot is `<=` the current timeslot, the upgrade is expired
   before this candidate is considered: release the new code (see §6.1) and clear
   `pending_upgrade`.
 5. **Validation code check**: This is the authoritative check. Verify the work
   result's `validation_code_hash` matches either the active
   `ParaInfo.validation_code` hash or the pending upgrade's
   code hash. If it matches neither, the candidate is rejected and an
   `AccumulateLog::InvalidCodeHash { hash }` event is emitted for this work package.
6. **Head data update + code upgrade check**: Writes the new `head_data` from the
   work digest into `ParaInfo` for the parachain and immediately checks whether the
   candidate was validated with the pending new PVF code. If so, activate the new
   code, release the old code (see §6.1), and clear `pending_upgrade`. This must
   happen here because later candidates from the same parachain in the same block
   may already use the new code.
7. **Process host-function calls from Refine**: Replay the `UpwardMessage`s carried in
   the work digest, applying the effects of each side-effect host function the PVF
   invoked during Refine (code upgrades, transfers, authorizer queue updates, validator
   key updates, etc.). See the side-effect host function table in §4.3 for the full list.
   This replay may itself emit further `AccumulateLog` events for the work package.

All `AccumulateLog` events emitted while processing a work package — the code-hash
mismatch from step 5 and any from the step 7 replay — are collected and appended to
`parachain_log[para_id]` as a single `LogEntry::Accumulate`. Every append to
`parachain_log[para_id]`, whether the `RefineLogEntry` from step 2 or this
`LogEntry::Accumulate`, is subject to the eviction rules below.

**Log pruning and eviction.** The per-parachain `parachain_log` is kept bounded
during Accumulate. When a candidate is processed, before its own refine/accumulate
entries are appended, entries whose inline timeslot is strictly less than the
candidate's lookup-anchor timeslot are pruned. The log
is additionally bounded to a 64 KiB total encoded size (not a fixed entry count),
so each entry is charged only its actual size; whenever a new entry would push the
log over 64 KiB, entries are evicted until it fits — the oldest `RefineLogEntry` is
dropped first (Refine failures are the most disposable, whereas an
`AccumulateLogEntry` records an actual on-chain state change and so carries
information worth retaining), and only once no refine
entry remains is the oldest entry overall dropped, so accumulate events are
retained under capacity pressure.

The core Accumulate logic is primarily **parachain bookkeeping**: updating head data,
tracking code upgrades, applying queued authorizer updates, and managing incoming
transfers.
Because selected work-reports are not replayed automatically, the service should checkpoint
after finishing each work-report so that progress survives any later out-of-gas or panic during
the same accumulation invocation.

### 5.2 Code Upgrade Lifecycle

Runtime (PVF) code upgrades follow a well-defined lifecycle using JAM's preimage
store (`solicit`/`provide`/`forget`) and the `xtpreimages` block extrinsic.

Validation code — both the active code and any pending upgrade code — lives in
`preimage_registry` (§3.1) like any other PVF-solicited preimage; two codes with the
same hash but different lengths are distinct entries. The service solicits a code
only when it isn't already solicited, so a code's referencer slot is held for
up to two independent reasons: it is the parachain's active/pending code (the
service's own reason), and/or the parachain solicited it itself. The latter is
recorded per-code by the `pinned` bit in `ParaInfo` (§3.1):

- **Parachain `solicit` of its active/pending code** sets the corresponding
  `pinned` bit. No extra state balance is charged — the code is already
  referenced.
- **Parachain `forget` of its active/pending code** clears that bit but does
  **not** release the referencer or forward a JAM `forget`: the service still
  needs the code available, so it stays solicited (§4.3).
- When a code **ceases to be active/pending** (activation, timeout reap, or a
  superseding request), this parachain's referencer is released unless its
  `pinned` bit is set (in which case the slot survives as an ordinary
  solicited preimage). The JAM `forget` itself is governed by the
  referencer multiplexing of §6.1, not by one parachain's state: it is
  forwarded only when the *last* referencer across all parachains is
  removed, so a code shared by several parachains is never dropped from JAM
  while any of them still references it.
- On **activation**, the pending code's `pinned` bit carries over to the
  now-active code.

```
Phase 1: Request
    Parachain calls request_code_upgrade(new_code_hash, len) during Refine.
    │
    ▼
Phase 2: Request Preimage
    Accumulate solicits the new code (see §6.1) and sets pending_upgrade
    with a deadline (current timeslot + UPGRADE_TIMEOUT). If the new code's
    footprint doesn't fit in the available state balance, the upgrade is
    rejected with AccumulateLog::InsufficientStateBalance.

    Overwriting an in-flight upgrade is allowed: if a different code is
    already pending, it is superseded — the old pending code's preimage is
    released (see §6.1) and replaced by the new request. Requesting the
    already-active code is a no-op.

    A candidate may both adopt its pending upgrade and request a new,
    different upgrade in the same block: Accumulate processes the upgrade
    activation (Phase 5a) first, then replays the upward messages, so the
    new request is armed against the just-activated code.
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
        validated with new_code_hash, sets validation_code_hash =
        new_code_hash, releases the old code (§6.1), and clears
        pending_upgrade.

    (b) Deadline exceeded: If the deadline (set in Phase 2) passes without
        the preimage becoming available or without any block using the new
        code, the upgrade is rejected on the next per-work-package
        accumulate for this parachain (see §5.1, step 2): the new code is
        released (§6.1) and pending_upgrade is cleared. The parachain
        continues with the old code.
```

**Key properties:**

- **No pre-checking needed**: PVM has no compilation bomb risk (unlike WASM), so there is
  no pre-checking vote. The code is accepted as soon as the preimage is available.
- **Dual-code cost**: During the transition period, both the old and new PVF code
   are counted against `ParaInfo.used_state_balance`. This incentivizes timely adoption.
   See the accounting model below.
- **Permissionless submission**: The preimage can be submitted by anyone — the collator,
  block author, or any third party. The JAM protocol validates the hash against the
  solicitation.
- **Timeout protection**: The deadline prevents parachains from indefinitely occupying
  preimage store space with unused code. `UPGRADE_TIMEOUT` is set to **24 hours**, which
  should be sufficient for the preimage to be submitted to JAM after solicitation.

### 5.3 Validator-Key Updates

A full `stagingset` (gray paper Safrole, eq. 108–112) is up to `1023 × 336 B ≈
336 KiB` — too large for a single work-report's `C_maxreportvarsize = 48 KiB`
result-blob budget, and JAM's `designate` accepts only the complete vector.
The Parachain Service therefore buffers chunks in `staged_validator_keys`
across multiple Asset Hub blocks (one chunk per block, since
`set_validator_keys` may be called at most once per Refine; see §4.3) until
Asset Hub signals completion via `is_last`.

When Accumulate replays a `SetValidatorKeys { keys, is_last }` upward message
it:

1. If `is_last == false`, appends `keys` to `staged_validator_keys`. The
   staging buffer is reserved worst-case in Asset Hub's baseline footprint
   (§6.1), but an append that would grow the buffer beyond its reserved capacity
   (the `1023`-key bound on `staged_validator_keys`) is rejected as invalid with
   `AccumulateLog::StagedValidatorKeysOverflow`, leaving the buffer unchanged.
2. If `is_last == true`, assembles the full set (prior buffer + `keys`) and
   clears the buffer. The service checks the assembled length against `valcount`:
   if valid it calls JAM `designate` with the set (which goes straight to
   `designate` and never persists in storage); otherwise `designate` is **not**
   called — the length check rejects the set and
   `AccumulateLog::DesignateRejected` is recorded against the Asset Hub `ParaId`.
   This also gives Asset Hub the abort path: `set_validator_keys(vec![], true)`
   yields a length-zero set, which the length check rejects, clearing the staging
   area.

A worst-case 1023-key rotation takes ~35 Asset Hub work packages (≈ 3.5
minutes at 6 s timeslots). State-balance accounting for the staging
buffer is covered in §6.1.

### 5.4 Service Self-Upgrade

Authority over Parachain Service code upgrades is held by **Asset Hub**. Asset Hub
triggers the upgrade via `parachain_service_upgrade` (§4.3), which emits
`UpwardMessage::UpgradeService` and is rejected by the Refine wrapper
for any other parachain. Accumulate forwards it to JAM's `upgrade`
host call after verifying that the new code's preimage is present.

```
Phase 1: Solicit
    Asset Hub calls solicit(new_code_hash, len) (§6.1).

Phase 2: Verify Solicit
    Asset Hub waits for its next block to be built on top of a JAM
    block whose state reflects the accumulated solicit, then reads
    its parachain_log via the validation inputs and confirms no
    AccumulateLog::InsufficientStateBalance for new_code_hash. If
    insufficient, Asset Hub aborts.

Phase 3: Upgrade
    Asset Hub calls parachain_service_upgrade(new_code_hash, ...).
    Accumulate forwards to JAM upgrade if the preimage is present;
    otherwise it logs AccumulateLog::ServiceUpgradePreimageMissing.

Phase 4: Activate
    On the next JAM invocation the Parachain Service runs under the
    new code.

Phase 5: Forget
    Asset Hub observes the new codehash in Parachain Service state
    and calls forget(old_code_hash, len) (§6.1).
```

---

## 6. Parachain Management

Parachain lifecycle and management is driven by the **Coretime chain**, which owns the
policy layer: ParaId allocation, deposits, and deciding when to create, overwrite, or
clean up a parachain's state.

The Parachain Service exposes four low-level, idempotent host functions that drive
state-balance management, registration, forced updates, and deregistration:

- `parachain_set_state_balance(para_id, new_total)` — set the parachain's quota
- `parachain_set_head(para_id, new_head)` — upsert head data
- `parachain_set_validation_code(para_id, new_validation_code_hash, new_validation_code_len)` — upsert validation code
- `parachain_clean_up(para_id)` — remove all per-parachain state

All four are Coretime-chain-only; the Parachain Service performs no rights-checking
of its own and in particular **does not enforce ParaId uniqueness** — the Coretime
chain is the sole authority on which `ParaId`s are live and who owns them.
`parachain_set_state_balance` is the sole creator of `ParaInfo` (see §6.1);
`parachain_set_head`, `parachain_set_validation_code`, and `parachain_clean_up`
silently no-op when invoked on a `ParaId` whose `ParaInfo` doesn't exist yet, so
Coretime must call `parachain_set_state_balance` first in any registration
sequence. On an existing `ParaId`, `parachain_set_head` /
`parachain_set_validation_code` simply overwrite (useful for forced recovery).

### 6.1 State-Balance Accounting

JAM bills each service for **everything it holds in state** — its storage key/value
entries, its solicited preimages, and the protocol-level service record itself — by
requiring the service to keep a minimum balance proportional to that footprint. The
Parachain Service inherits that obligation for the *aggregate* footprint of all
parachains it hosts, and re-attributes it **per parachain** via `used_state_balance`.

The per-parachain footprint includes everything the service stores under this
parachain's `ParaId` — `ParaInfo`, solicited preimages, the `parachain_log` reserve,
slots in shared structures like `preimage_registry`, and any future per-`ParaId`
state.

Each parachain is billed as if it were the **sole user** of every data structure it
touches in service state: its `used_state_balance` is the sum of each structure's
footprint computed as though the stored value held only this parachain's contribution.
JAM's footprint formulas (Gray Paper, *Account Footprint and Threshold Balance*) are
`34 + |value| + |key|` per general-storage entry and `81 + z` per solicited preimage of
length `z`.
Shared structures like `preimage_registry` end up over-collateralized — every
referencer pays for a full entry — but existing parachains' contributions never need
recomputing when the referencer set changes.

#### Total balance management (Coretime chain only)

The Coretime chain is the sole authority on `total_state_balance`. It calls
`parachain_set_state_balance(para_id, new_total)` to set the value — at registration
to create the initial budget (see §6.2), and at any later point to grant additional
headroom for any additional state requirements or to reclaim slack once the parachain stabilizes.

`parachain_set_state_balance` is the sole creator of `ParaInfo`. Called on a
previously-unused `ParaId`, it creates a fresh entry with
`total_state_balance = new_total`, `used_state_balance = baseline_footprint`, and
the other fields uninitialized (to be filled in by subsequent `parachain_set_head` /
`parachain_set_validation_code` calls in the same registration sequence). Called on
an existing `ParaId`, it overwrites `total_state_balance` in place.

In either case the call is applied only if `new_total >= used_state_balance` (so
the Coretime chain cannot strand currently-paid-for state by under-funding the
parachain). Otherwise no state change happens and an
`AccumulateLog::StateBalanceUpdateRejected { attempted, current_total, current_used }`
is appended to the parachain log so the Coretime chain can observe the rejection
and size a retry. To free state balance, the parachain (or the Coretime chain via
a forced update / clean-up; see §6) must first reduce `used_state_balance` by
releasing preimages.

Verifying the user has enough balance to cover at least the baseline is the Coretime
chain's responsibility, done before starting the registration sequence.

Deposits, sizing, and refunds are owned end-to-end by the Coretime chain; end users
interact with it via its usual extrinsics, and the Coretime chain reflects those
interactions into the Parachain Service via `parachain_set_state_balance`.

#### Preimage multiplexing

JAM allows only one `(hash, len)` solicitation per service. The Parachain Service is
a single service hosting many parachains, so it multiplexes via `preimage_registry`:
each entry records the set of `ParaId`s referencing the hash. JAM `solicit` is
called when the set transitions empty → non-empty; JAM `forget` is called when it
transitions back to empty.

Releasing an *available* preimage takes **two steps**. A JAM `forget` does not
delete it — it marks the request unavailable, and only a **second** `forget`, no
earlier than `C_expungeperiod = 19 200` timeslots (~32 h) later, actually expunges
it. The service keeps no bookkeeping for this. When a `forget` removes the last
referencer without expunging the preimage, Accumulate appends an
`AccumulateLog::ForgetAgainAt { hash, len, due }`, where `due = now +
C_expungeperiod`, and leaves the last referencer in `referencers` — still charged
the full footprint. The parachain calls `forget(hash, len)` again once the
timeslot is *strictly after* `due` to complete the expunge and free the footprint.

A preimage that was solicited but **never provided** to JAM is different: a single
`forget` of its last referencer drops the request outright - there is nothing to
expunge, so the footprint is freed immediately with no `ForgetAgainAt`.

**Rescue.** During the ~32 h window between the two forgets, JAM still holds the
blob, so a `solicit` can bring the request back to available. The service does this
automatically: if a parachain references an entry whose last referencer is awaiting
expunge, Accumulate re-forwards JAM `solicit` and the preimage serves lookups again.
The rescuing parachain becomes the entry's sole referencer, and the parachain that
was awaiting expunge is dropped and refunded (it had already forgotten; it was only
being held as the stand-in for the pending second forget).

A rescue does **not** reset the expunge deadline: the gate on the next `forget` is
still measured from the *original* unrequest, so a `forget` at or before that `due`
is a no-op that must be retried strictly after `due` reported by `ForgetAgainAt`.
And when that `forget` does fire, it does not expunge - it only marks the preimage
unavailable again.

Applying §6.1's sole-user rule, a single referencer's **preimage footprint** is the
sum of two JAM entries: the **preimage request** (`81 + len`) and its
**`preimage_registry` entry** — `34 + |value| + |key|` = `34 + 5 + 37` (the per-item
overhead, a singleton `{ParaId}` referencer set of 5 B, and the storage key of 37 B (1 B map tag + 32 B hash + 4 B len)). That is **157 + len** bytes per referencer, even though the on-chain
entry may hold many referencers.

#### Sizing the baseline footprint

`baseline_footprint` is the worst-case state cost of an empty parachain — the
`(ParaId, ParaInfo)` entry plus the `(ParaId, parachain_log[para_id])` entry, with
every bounded field SCALE-encoded at its maximum so the value is static across the
parachain's lifetime. JAM charges `34 + |value| + |key|` bytes per general-storage
entry. Taking `ParaId = u32` (4 B), `Hash = 32 B`, `Timeslot = u32` (4 B), and
`Balance = Compact<u128>` sized at its worst case of 17 B:

`(ParaId, ParaInfo)` entry:

```
JAM per-item overhead                                              =      34 B
map tag                                                            =       1 B
ParaId (key)                                                       =       4 B
head_data: BoundedVec<u8, 4096> = 2 (compact len) + 4096           =   4 098 B
validation_code: ValidationCode = 32 (hash) + 4 (len) + 1 (pinned) =      37 B
pending_upgrade: Option<(ValidationCode, Timeslot)> = 1 + 37 + 4    =      42 B
total_state_balance: Compact<Balance>                              =      17 B
used_state_balance: Compact<Balance>                               =      17 B
                                                                       -------
                                                                       4 250 B
```

`(ParaId, parachain_log[para_id])` entry — value + key bounded by a flat 64 KiB cap,
with JAM's per-item overhead on top:

```
The log value is bounded by exact encoded size (entries sized by actual SCALE
length, not worst-case). The 64 KiB cap covers every log element plus the
vector's own length prefix; JAM's 34 B per-item overhead and the 5 B storage key
(1 B map tag + 4 B ParaId) sit on top, so the service reserves a flat
64 KiB + 34 B + 5 B regardless of current contents.

JAM per-item overhead                                              =      34 B
storage key (1 B map tag + 4 B ParaId)                             =       5 B
parachain_log value (flat cap): 64 KiB                             =  65 536 B
                                                                      -------
                                                                     65 575 B
```

**`baseline_footprint = 4 250 + 65 575 = 69 825 B`** per parachain.

#### Asset Hub baseline footprint

Asset Hub additionally owns the service-global state items as privileged
caller; its `total_state_balance` must cover them, provisioned at genesis. Each
item follows the same JAM `34 + |value| + |key|` per-entry rule (§6.1): the two
`CoreIndex`-keyed maps are billed one storage entry per core, the `BoundedVec`s as
a single entry each. Taking `CoreCount = 341`, `AuthorizerHash = 32 B`,
`ServiceId = 4 B`, `Memo = 128 B`, `CoreIndex = 4 B`, authorizer-queue length = 80,
and `Amount = Compact<u128>` sized at its worst case of 17 B:

```
staged_validator_keys: BoundedVec<ValidatorKey, 1023>  — 1 entry
  34 + 1 (key) + 2 + 1023 × 336                                         = 343 765 B
pending_authorizer_queues: Map<CoreIndex, BoundedVec<AuthorizerHash, 80>>  — per core
  341 × (34 + 5 (key) + 2 + 80 × 32)                                    = 886 941 B
pending_authorizer_cores: BoundedVec<(CoreIndex, Timeslot), 341>  — 1 entry
  34 + 1 (key) + 2 + 341 × (4 + 4)                                      =   2 765 B
incoming_transfers: BoundedVec<(ServiceId, Amount, Memo), 1000>  — 1 entry
  34 + 1 (key) + 2 + 1000 × (4 + 17 + 128)                              = 149 037 B
                                                                           ---------
                                                                         1 382 508 B
```

**Asset Hub baseline footprint ≈ 1.32 MiB**, added on top of the generic
per-para baseline.

#### Key-Value storage footprint

Each `(ParaId, key) -> value` entry in `key_value_storage` pays the JAM
sole-user cost `34 + |value| + |storage_key|`, where the JAM storage key
composes the map tag, the parachain id, and the SCALE-encoded user key:

```
kv_entry_footprint(k, v) = 34 (JAM overhead)
 + compactLen(v) + v (SCALE Vec<u8> value)
 + 1 (map tag) + 4 (ParaId) (per §3.1 storage-key encoding)
 + compactLen(k) + k (SCALE Vec<u8> user key)
 = 39 + compactLen(k) + k + compactLen(v) + v
```

A `kv_set` computes the change in `used_state_balance` — the new entry's
footprint, or `compactLen(new_v) + new_v − compactLen(old_v) − old_v` when
overwriting an existing key. The old value's length is recovered without
materializing the old value: since it is a SCALE-encoded `Vec<u8>`, reading
just the first 4 bytes (via JAM `read`'s offset/length) is enough to decode
the `Compact<u32>` length prefix. When the change is positive it must fit
within `total_state_balance` before the write is applied; when it is negative
(an overwrite with a smaller value) the freed balance is credited back. A
`kv_remove` refunds `kv_entry_footprint(k, v)` for the removed entry.

#### Write-time invariant

Every mutation that would grow `used_state_balance` is guarded by a headroom
pre-check against `total_state_balance` before the write. On insufficient
headroom the write is skipped and `AccumulateLog::InsufficientStateBalance` is
appended to the parachain log; otherwise the write is applied and
`used_state_balance` is bumped atomically. Baseline-covered state is
pre-charged and needs no per-write check.

Because every growth is pre-checked, a state write never fails on balance
grounds. A defensive write-time balance failure indicates a bookkeeping bug and
can leave the entire service stuck until manual intervention.

### 6.2 Registration

Registration is the composition of `parachain_set_state_balance`,
`parachain_set_head`, and `parachain_set_validation_code` on a previously-unused
`ParaId`, in that order:

```
Coretime chain
    │  Account submits registration: genesis head + validation code hash + len.
    │  Coretime sizes the deposit per §6.1, allocates the ParaId, and calls:
    │      parachain_set_state_balance(para_id, total)
    │      parachain_set_head(para_id, genesis_head)
    │      parachain_set_validation_code(para_id, validation_code_hash, validation_code_len)
    ▼
Parachain Service (Accumulate)
    │  ParaInfo created (rejected if total < baseline), head_data set,
    │  validation code solicited and its footprint charged (§6.1).
    ▼
User submits the validation code preimage to JAM (xtpreimages extrinsic).
Parachain goes live on its assigned core once the preimage is available.
```

Registration does **not** wait for the preimage.

### 6.3 Forced Updates (Recovery)

The same two host functions also handle exceptional recovery — e.g. unsticking a chain
whose last included block cannot be built on, or swapping in a new PVF outside the normal
upgrade lifecycle:

- `parachain_set_head(para_id, new_head)` overwrites `ParaInfo.head_data`.
- `parachain_set_validation_code(para_id, new_hash, new_len)` sets
  `ParaInfo.validation_code` to `Some(new_hash)`, solicits `new_hash`, and clears any
  `pending_upgrade`. `used_state_balance` grows by `preimage_footprint(new_len)`
  to hold the new validation code. The displaced validation codes (the old active
  code and any pending code) are released via the normal `forget` step (§6.1).
  Each keeps its footprint charged until the parachain calls `forget` again to
  complete the two-step release. So while those releases are in flight the
  parachain holds the new validation code plus every not-yet-forgotten displaced
  validation code. That is two validation codes when there was no pending
  upgrade, three when there was. `total_state_balance` must cover whatever is
  concurrently held. The call is rejected with
  `AccumulateLog::InsufficientStateBalance` if the new footprint wouldn't fit, so
  Coretime must raise `total_state_balance` first (in the same batch) when needed.

```
Coretime chain
    │  Verifies the rights of the caller
    │  Calls parachain_set_state_balance(para_id, new_total) if needed
    │  Calls parachain_set_head(para_id, new_head) OR
    │        parachain_set_validation_code(para_id, new_validation_code_hash, new_validation_code_len)
    ▼
Parachain Service (Accumulate)
    │  Applies the change, re-soliciting/forgetting preimages and adjusting
    │  used_state_balance as described above.
```

### 6.4 Clean-up (Deregistration)

```
Coretime chain
    │  Verifies the rights of the caller
    │  Calls parachain_clean_up(para_id)
    ▼
Parachain Service (Accumulate)
	│  Rejects with `AccumulateLog::TooMuchStateHeld` unless used_state_balance
	│  is exactly BASELINE_FOOTPRINT + preimage_footprint(validation_code)
	│  + preimage_footprint(pending_upgrade code, if any) — i.e. the parachain
	│  has already released all other solicited preimages and key_value_storage.
	│  Otherwise forgets the validation code(s). If any cannot be expunged yet
	│  (JAM's two-step forget; see §6.1), sets ParaInfo.is_deregistering = true
	│  and stops. Once expungeable, drops parachains[para_id]
	│  and parachain_log[para_id].
```

Requiring the parachain to drain its own extra state first keeps clean-up bounded:
the service only ever has to forget the two validation codes, never an unbounded
set of solicited preimages or KV entries.

While `is_deregistering` is set the service rejects every work package for the
parachain (§5.1), so no new state accrues. Each not-yet-expungeable validation
code emits a `ForgetAgainAt { .., due }` log (§6.1). The Coretime chain retries
the call once the timeslot is strictly past the latest such `due`, and the
parachain is fully removed. This keeps all follow-up in a single host call rather
than tracking per-preimage `forget` deadlines.

Coretime also handles deposit refund and any economic unwinding according to its
own policy.

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

Each parachain supplies its own authorizer — the Parachain Service does
not prescribe a specific one. The only constraint the service imposes is
that the authorizer's config blob begins with a `Vec<ParaId>` matching
the work package's items (§3.2). What follows is one AURA-style collator-set
authorizer, sketched for demonstration purposes only.

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
3. Read the **anchor timeslot** from the refinement context.
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
structured complaint against the offending collator in the parachain log, which can
then be read by the parachain's slashing logic.

#### Collator Set Rotation Flow

```
Parachain runtime
    │  Decides to rotate collator set (e.g. via session change)
    │  Sends XCM to Coretime chain with new collator set root + size
    ▼
Coretime chain
    │  calls set_authorizer_queue(core, authorizers, jam_slot)
    │  (new authorizer hashes computed from same code + updated config)
    ▼
Parachain Service (Accumulate)
    │  assign(core, new_queue) — retaining the current assigner
    │  New authorizer hashes enter the pool via queue rotation
    ▼
Pool (up to 8 entries)
    │  Old authorizer hashes drain out over ~8 blocks (48s)
    │  New ones rotate in
```

### 7.2 On-Demand Parachains

On-demand coretime is not a special case for the Parachain Service — it is handled
entirely by the **Coretime chain**. When someone buys a single-slot coretime allocation,
the Coretime chain just calls `set_authorizer_queue(core, queue, jam_slot)` with a
near-term `jam_slot` to install the buyer's authorizer on the target core for the
duration of that slot. The
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
work on JAM because the work digest output is too small to carry message payloads on-chain.
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

## TODOS

- 
