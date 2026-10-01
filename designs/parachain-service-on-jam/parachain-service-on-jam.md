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
   - 4.2 [Validation Code Entry Point](#42-validation-code-entry-point)
   - 4.3 [Host Functions & PVM Imports](#43-host-functions-pvm-imports)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
   - 5.1 [What Accumulate Does](#51-what-accumulate-does)
   - 5.2 [Parachain Code Upgrade Lifecycle](#52-parachain-code-upgrade-lifecycle)
   - 5.3 [Validator-Key Updates](#53-validator-key-updates)
   - 5.4 [Service Self-Upgrade](#54-service-self-upgrade)
   - 5.5 [Parachain Head Commitment](#55-parachain-head-commitment)
6. [Parachain Management](#6-parachain-management)
   - 6.1 [State-Balance Accounting](#61-state-balance-accounting)
   - 6.2 [Registration](#62-registration)
   - 6.3 [Forced Updates (Recovery)](#63-forced-updates-recovery)
   - 6.4 [Clean-up (Deregistration)](#64-clean-up-deregistration)
7. [Authorization & Coretime](#7-authorization-coretime)
   - 7.1 [Authorizer Design: AURA Example](#71-authorizer-design-aura-example)
   - 7.2 [On-Demand Parachains](#72-on-demand-parachains)
8. [Messaging](#8-messaging)
9. [References](#9-references)

---

## 1. Overview

This document describes the architecture of the **Parachain Service**, a JAM service that implements
Polkadot's parachain host functionality. The Parachain Service is the JAM successor to the current
Polkadot relay-chain parachain host, mapping collation, validation, availability, and finality onto
JAM's Collect-Refine-Join-Accumulate (CRJA) computation model.

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

This document does **not** cover JAM fundamentals in depth. Readers are assumed to be familiar with
the [JAM Gray Paper](https://graypaper.com) concepts (services, work packages, refine, accumulate,
guarantors, etc.).

### Conventions

`hash` and the `Hash` type mean blake2b-256. The only other hash used here is `keccak_256`, for the
head-commitment tree elements in §5.5.

---

## 2. Architecture Overview

The Parachain Service maps the current relay chain's parachain host logic onto JAM's
two execution domains:

- **Refine (in-core)**: Executes `jam_validate_block`, the validation-code execution that backing
  validators currently perform. Guarantors run the validation code against the PoV to verify the
  parachain block candidate. This replaces the current backing subsystem.
- **Accumulate (on-chain)**: Enacts the candidate by updating head data, processing signals,
  and managing channels and code upgrades. This replaces the current inclusion pallet logic.

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
[Refine]      IN-CORE: Guarantors run the Parachain Service's Refine, which calls the validation
              code to verify the work package.
              Stateless, off-chain, metered via PVM gas.
              Output: per-item work-digests plus authorization/export metadata, assembled into a Work Report.
    │
    ▼
[Join]        The Work Report is submitted on-chain.
              JAM validators attest (guarantee) its correctness. Availability of the work package is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate records the new parachain head, applies
              the validation code's upward messages (code upgrades, outbound transfers,
              authorizer updates, etc.), and queues incoming transfers from other services.

```

## 3. The Parachain Service

The Parachain Service is a JAM service whose code implements the on-chain logic of the parachain
host. It holds all per-parachain state and drives the CRJA pipeline for every registered parachain.
It uses the fixed JAM service ID **1337**.

The Parachain Service is expected to be an **always-accumulate** service in the Gray Paper sense.
Even in blocks where no parachain candidate becomes available, it needs an accumulation step to
apply scheduled authorizer queue changes once they are due (§5.1). These must take effect
without waiting for a parachain block.

Together with the privileged host calls the service forwards, this requires four registrations
in JAM's protocol state: membership in the always-accumulate set (with a gas allowance), being
the **delegator** (required for `designate`, §5.3), being the registered **assigner** of every
core it manages (required for `assign`, §7.1), and being the **registrar** (required for
`CreateService`'s `desired_id`, §3.3).

### 3.1 Service State Layout

The service state is a key-value store. Logically it contains:

```rust
// Top-level service state (conceptual, not final)
struct ParachainServiceState {
    /// All registered parachains and their current metadata.
    parachains: Map<ParaId, ParaInfo>,

    /// Incoming transfers recorded for Asset Hub. See §5.1.
    incoming_transfers: Map<BucketId, IncomingTransfers>,

    /// Endpoints of the `incoming_transfers` queue. See §5.1.
    incoming_transfer_buckets: IncomingTransferBuckets,

    /// Per-parachain log of Refine failures and Accumulate events, capped at
    /// 64 KiB. See §5.1.
    parachain_log: Map<ParaId, Vec<(Timeslot, LogEntry)>>,

    /// Scheduled-but-unapplied `assign` payloads, keyed by core.
    pending_assigns: Map<CoreIndex, PendingAssign>,

    /// Each core with a pending assign, paired with the timeslot it is due at.
    /// See §5.1.
    pending_assign_cores: BoundedVec<(CoreIndex, Timeslot), CoreCount>,

    /// Every preimage the service has solicited from JAM, keyed by hash and
    /// length, with the parachains referencing it. See §6.1.
    preimage_registry: Map<(Hash, u32), PreimageEntry>,

    /// Validator-key set being assembled chunk by chunk by
    /// `SetValidatorKeys`. See §5.3.
    staged_validator_keys: BoundedVec<ValidatorKey, 1023>,

    /// Per-parachain key/value store. See §6.1 for the per-entry footprint.
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
    /// Events recorded while accumulating one of this parachain's work
    /// packages. See §5.1.
    entries: Vec<AccumulateLog>,
}

/// Why Refine failed, as recorded in `parachain_log` (§5.1).
enum RefineLog {
    /// The validation code is not available at the lookup-anchor.
    /// See §4.1 step 4.
    ValidationCodeLookupFailed,
    /// Payload the validation code passed to `report_error`. See §4.2.
    Opaque(BoundedVec<u8, 1024>),
    /// `SetValidatorKeys` was called more than once in a single Refine
    /// invocation. See §4.3, §5.3.
    SetValidatorKeysRepeated,
    /// A `SetValidatorKeys` chunk carried more than 30 keys. See §4.3, §5.3.
    TooManyValidatorKeys,
    /// The validation code emitted more than `MAX_UPWARD_MESSAGES` upward messages in a
    /// single Refine invocation. See §4.3.
    TooManyUpwardMessages,
    /// The parachain's 40 KiB upward-message budget was exceeded. See §4.3.
    UpwardMessagesTooLarge,
    /// The validation code sent an upward message reserved for another
    /// parachain, or named a `para_id` it may not act for. See §4.3.
    RestrictedHostFunction,
    /// An `AssignCore` carried a queue of invalid length. See §3.3.
    InvalidAuthorizerQueue,
    /// The encoded `ParachainWorkDigest` and auth trace would exceed the Gray
    /// Paper's 48 KiB. See §4.1.
    RefineOutputTooLarge,
    /// The validation code did not call both `set_parent_head_hash` and
    /// `set_head` exactly once. See §4.2.
    InvalidHeadDeclaration,
    /// `set_head` was called with head data beyond the 4 KiB `HeadData` bound.
    /// See §4.3.
    HeadDataTooLarge,
    /// An `AssignCore` named a core at or above `C_corecount`. See §3.3.
    InvalidCoreIndex,
    /// A `SetKV` or `RemoveKV` carried an empty key, or a `SetKV` an empty value.
    /// See §3.3.
    EmptyKVKeyOrValue,
}

/// The two phases of a `RequestCodeUpgrade` (see §5.2).
enum CodeUpgradePhase {
    /// Name the code a later `Apply` may switch to.
    Announcement,
    /// Switch `validation_code` to the announced code.
    Apply,
}

/// Why a state-balance reservation failed (see §6.1).
enum InsufficientBalanceReason {
    /// A `Solicit` of the preimage with `hash` and `len`.
    Solicit { hash: Hash, len: Compact<u32> },
    /// A `SetKV` write to `key_value_storage`, identified by the hash of its
    /// key.
    SetKV { key_hash: Hash },
    /// A `staged_validator_keys` append.
    StagedValidatorKeys,
    /// An `incoming_transfers` or `incoming_transfer_buckets` write.
    IncomingTransfer,
    /// A `ParaInfo` write: head, registration, forced code or announced upgrade.
    ParaInfo,
}

/// Why `ParachainSetStateBalance` was rejected (see §6.1).
enum StateBalanceRejection {
    /// `attempted < current_used`.
    BelowUsed { current_total: Compact<Balance>, current_used: Compact<Balance> },
    /// `para_id` is deregistering (§6.4).
    ParachainIsDeregistering,
}

enum AccumulateLog {
    /// Available state balance insufficient for the operation described by
    /// `reason`. See §6.1.
    InsufficientStateBalance { reason: InsufficientBalanceReason },
    /// `ParachainSetStateBalance { para_id, new_total: attempted }` was rejected for
    /// `reason`. See §6.1.
    StateBalanceUpdateRejected {
        para_id: ParaId,
        attempted: Compact<Balance>,
        reason: StateBalanceRejection,
    },
    /// JAM `designate` rejected the assembled validator-key set of length
    /// `len`. See §5.3.
    DesignateRejected { len: Compact<u32> },
    /// A `SetValidatorKeys` chunk would overflow `staged_validator_keys`.
    /// See §5.3.
    StagedValidatorKeysOverflow,
    /// Asset Hub does not reference the new code's preimage, or it is not
    /// available for lookup. See §5.4.
    ServiceUpgradePreimageMissing { code_hash: Hash },
    /// An `Announcement` whose code is not available. See §5.2.
    CodeUpgradeNotAvailable { hash: Hash, len: Compact<u32> },
    /// An `Apply` that does not match the standing announcement. See §5.2.
    CodeUpgradeNotAnnounced { hash: Hash, len: Compact<u32> },
    /// A `Forget` naming code that is still in use. See §6.1.
    CanNotRemoveCode { hash: Hash, len: Compact<u32> },
    /// JAM rejected an `assign` because this service is no longer the core's
    /// assigner. See §5.1.
    CoreNotAssignable { core: CoreIndex },
    /// The JAM `transfer` for the `TransferOut` with this `id` failed. See §5.1.
    TransferFailed { id: Compact<u64>, error: TransferError },
    /// A `forget` left the preimage in place. `Forget` it again after `due`.
    /// See §6.1.
    ForgetAgainAt { hash: Hash, len: Compact<u32>, due: Timeslot },
    /// A `Forget` or `RemoveServiceStorage` on a supervised service's store
    /// failed.
    ServiceStoreFailed { service: ServiceId, error: ServiceStoreError },
    /// A `Service`-targeted `Solicit` failed.
    ServiceSolicitFailed { service: ServiceId, error: ServiceSolicitError },
    /// An `EjectService` failed.
    ServiceEjectFailed { service: ServiceId, error: ServiceEjectError },
    /// A `SetServiceSupervisor` failed.
    ServiceSupervisorFailed { service: ServiceId, error: ServiceSupervisorError },
    /// Outcome of the `CreateService` with this `id`.
    ServiceCreation { id: Compact<u64>, result: ServiceCreationResult },
    /// `ParachainCleanUp` was rejected because the parachain still holds state
    /// beyond its baseline and validation code(s). See §6.4.
    TooMuchStateHeld,
}

/// What a `Forget` acts on: a registered parachain's share of this service's
/// own store, or a supervised service's store.
enum Target {
    Parachain(ParaId),
    Service(ServiceId),
}

/// Why a `Forget` or `RemoveServiceStorage` against a supervised service's
/// store failed.
enum ServiceStoreError {
    /// The named service does not exist.
    UnknownService,
    /// The Parachain Service is not its effective supervisor.
    NotSupervised,
    /// A `Forget` naming a preimage the target never requested.
    NotRequested,
}

/// Why a `Service`-targeted `Solicit` failed.
enum ServiceSolicitError {
    UnknownService,
    NotSupervised,
    /// The request would leave the target below its threshold balance.
    TargetCannotAfford,
    /// The target already has a live request for this preimage that is not
    /// awaiting re-solicitation.
    AlreadySolicited,
}

/// Why an `EjectService` failed.
enum ServiceEjectError {
    UnknownService,
    NotSupervised,
    /// The service still holds storage or preimage requests, and must be
    /// emptied first.
    NotEmpty,
    /// The service was created in this timeslot.
    CreatedThisSlot,
    /// The Parachain Service named itself.
    TargetIsSelf,
}

/// Why a `SetServiceSupervisor` failed.
enum ServiceSupervisorError {
    /// The named service does not exist.
    UnknownService,
    /// The proposed new supervisor does not exist.
    UnknownNewSupervisor,
    /// The Parachain Service is not its effective supervisor.
    NotSupervised,
}

/// How a `CreateService` turned out.
enum ServiceCreationResult {
    /// Succeeded, carrying the id JAM assigned, which may differ from
    /// `desired_id`.
    Created(ServiceId),
    /// The Parachain Service cannot fund the new service.
    CannotAfford,
    /// A `desired_id` in the protected range that is already in use.
    IdTaken,
}

/// Why a JAM `transfer` replaying a `TransferOut` failed. See §5.1 step 6.
enum TransferError {
    /// `source` is not a known service.
    UnknownSource,
    /// `dest` is not a known service.
    UnknownDestination,
    /// The service is not `source`'s effective supervisor. Only its own regular
    /// balance is exempt. Takes precedence over `DestinationNotSupervised`.
    SourceNotSupervised,
    /// A plain move to another service needs the service to be `dest`'s
    /// effective supervisor. Also covers an identity write (`source == dest`
    /// with both selectors equal).
    DestinationNotSupervised,
    /// The supplied gas is below `dest`'s `min_memo_gas`.
    GasBelowDestinationMinimum,
    /// The `source` service cannot cover `amount`, either because the debited
    /// balance is too small or because the transfer would leave it below its
    /// threshold balance.
    InsufficientServiceBalance,
}

struct PreimageEntry {
    /// Parachains currently referencing this preimage. Bounded by the
    /// protocol-level maximum number of parachains.
    referencers: BoundedBTreeSet<ParaId>,
}

/// A scheduled JAM `assign` for one core, where `AUTH_QUEUE_SIZE = 80` is the
/// number of slots `assign` consumes. See §7.1.
struct PendingAssign {
    /// The authorizer set, up to `AUTH_QUEUE_SIZE` hashes. See §7.1.
    queue: BoundedVec<AuthorizerHash, AUTH_QUEUE_SIZE>,
    assigner: Option<ServiceId>,
}

/// Key of one `incoming_transfers` bucket. See §5.1.
type BucketId = u64;

/// One bucket of the `incoming_transfers` queue, in arrival order.
type IncomingTransfers = BoundedVec<IncomingTransfer, MAX_TRANSFERS_PER_BUCKET>;

/// One recorded incoming transfer.
struct IncomingTransfer {
    source: ServiceId,
    amount: Compact<Amount>,
    /// Whether the amount went to the supervisor balance (JAM `destsupervisor`).
    to_supervisor_balance: bool,
    memo: Memo,
}

/// The occupied bucket ids are `first_bucket ..= last_bucket`. See §5.1.
struct IncomingTransferBuckets {
    first_bucket: BucketId,
    last_bucket: BucketId,
    /// Total queued transfers across every bucket.
    count: u32,
}

/// Parachain head data, capped at 4 KiB. See §6.1.
type HeadData = BoundedVec<u8, { 4 * 1024 }>;

/// Fixed 128-byte transfer memo, matching Gray Paper `C_memosize = 128`.
type Memo = [u8; 128];

/// A validation code reference: its hash plus its SCALE-encoded byte length.
struct ValidationCodeRef {
    hash: ValidationCodeHash,
    len: u32,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Currently active validation code, or `None` for a freshly-registered
    /// parachain. See §6.
    validation_code: Option<ValidationCodeRef>,
    /// Announced code upgrade. See §5.2.
    announced_upgrade: Option<ValidationCodeRef>,
    /// State balance allocated to this parachain. See §6.1.
    total_state_balance: Compact<Balance>,
    /// State balance currently charged to this parachain. See §6.1.
    used_state_balance: Compact<Balance>,
    /// Whether the parachain is being deregistered. See §6.4.
    is_deregistering: bool,
}
```

#### Storage key encoding

Each storage item (a top-level `Map` or a singleton) is assigned a distinct
**1-byte tag** identifying it within the service's JAM storage. The full JAM
storage key is `[tag: u8] || SCALE-encoded logical key`: the tag followed by the
encoded map key for a map entry, and the tag alone for a singleton. The exception is
`key_value_storage`, whose user key is appended as sent, without a length prefix:
`0x08 || para_id || key`.

| Tag | Storage item |
|--------|------------------------------|
| `0x00` | `parachains` |
| `0x01` | `parachain_log` |
| `0x02` | `pending_assigns` |
| `0x03` | `pending_assign_cores` |
| `0x04` | `preimage_registry` |
| `0x05` | `staged_validator_keys` |
| `0x06` | `incoming_transfers` |
| `0x07` | `incoming_transfer_buckets` |
| `0x08` | `key_value_storage` |

### 3.2 Work Items

Each work package submitted to the Parachain Service contains one or more **work items**.
For the Parachain Service, a work item represents one parachain candidate. Its **payload**
carries the validation code hash, and the **PoV** is passed as a work-item extrinsic.

```rust
struct ParachainCandidate {
    /// The hash of the currently active validation code. See §4.1.
    validation_code: ValidationCodeHash,
}
```

Initially, each work package will contain a single work item (one parachain candidate).
Support for multiple items per package may be added later.

The `ParaId` for each work item is **not** stored in the work item itself. Instead, it is
sourced from the authorizer config, which is pinned by the Coretime chain (see §7.1). The
Parachain Service enforces that every authorizer config begins with a `Vec<ParaId>` whose
length matches the number of work items in the package, so that work item `item_index` is
authoritatively bound to `authorized_paras[item_index]`. Refine reads this prefix via
`fetch` and uses it to populate `ParachainWorkDigest.para_id`.

### 3.3 Work Digest

The Parachain Service's Refine function returns a parachain work digest per work item.
This digest is forwarded to Accumulate. From the service's perspective, Refine either
succeeds or fails:

```rust
/// The Parachain Service's Refine output for one parachain candidate.
enum ParachainWorkDigest {
    Ok {
        /// The parachain this digest belongs to.
        para_id: ParaId,
        /// Hash of the validation code Refine used to check the candidate.
        validation_code: ValidationCodeHash,
        /// Hash of the parent head data this candidate was built on top of.
        parent_head_hash: Hash,
        /// New head data produced by the parachain block.
        head_data: HeadData,
        /// Upward messages in the order they were emitted. See §5.1 step 6.
        upward_messages: Vec<UpwardMessage>,
        /// The work package's lookup-anchor timeslot.
        lookup_anchor: Timeslot,
    },
    /// Refine failed. See §4.1.
    Err {
        /// The parachain this failure belongs to.
        para_id: ParaId,
        /// Hash of the validation code the candidate names. See §5.1 step 2.
        validation_code: ValidationCodeHash,
        error: RefineLog,
    },
}

enum UpwardMessage {
    /// Drive a parachain code upgrade. See §5.2.
    RequestCodeUpgrade {
        hash: ValidationCodeHash,
        len: Compact<u32>,
        phase: CodeUpgradePhase,
    },
    /// Request a preimage, charged to the target's state balance. See §6.1 for a
    /// `Parachain` target. A `Service` target requests into that service's own
    /// store and is **Asset Hub only**.
    Solicit { target: Target, hash: Hash, len: Compact<u32> },
    /// Destroy an empty supervised service, crediting its balances to this
    /// service. **Asset Hub only.**
    EjectService { service: ServiceId },
    /// Hand a supervised service to another supervisor, or to itself to set it
    /// free. **Asset Hub only.**
    SetServiceSupervisor { service: ServiceId, new_supervisor: ServiceId },
    /// Create a service supervised by this one, funded from this service's
    /// balance. `id` is a caller-supplied identifier, echoed back in the
    /// `ServiceCreation` log entry so Asset Hub can match the outcome to its
    /// request. **Asset Hub only.**
    CreateService {
        code_hash: Hash,
        len: Compact<u32>,
        min_item_gas: u64,
        min_memo_gas: u64,
        id: Compact<u64>,
        /// Index to create the service at, in JAM's protected range. Only
        /// honoured while the Parachain Service is the registrar (§3).
        desired_id: Option<ServiceId>,
        source_supervisor_balance: bool,
        new_supervisor_balance: bool,
    },
    /// Release a previously solicited preimage of `target`. See §6.1. A
    /// `Service` target is **Asset Hub only**.
    Forget { target: Target, hash: Hash, len: Compact<u32> },
    /// Delete `key` from a supervised service's own storage. **Asset Hub only.**
    RemoveServiceStorage { service: ServiceId, key: Vec<u8> },
    /// Upsert `key_value_storage[(para_id, key)] = value`. An empty `key` or `value`
    /// aborts Refine with `Err(RefineLog::EmptyKVKeyOrValue)`. See §6.1.
    SetKV { key: Vec<u8>, value: Vec<u8> },
    /// Remove `key_value_storage[(para_id, key)]`. An empty `key` aborts Refine with
    /// `Err(RefineLog::EmptyKVKeyOrValue)`. See §6.1.
    RemoveKV { para_id: ParaId, key: Vec<u8> },
    /// Transfer balance via JAM `transfer`. `id` is a caller-supplied
    /// identifier, echoed back in `TransferFailed` so Asset Hub can match a
    /// failure to its request. See §5.1. **Asset Hub only.**
    TransferOut {
        source: Option<ServiceId>,
        dest: ServiceId,
        amount: Compact<Amount>,
        id: Compact<u64>,
        source_supervisor_balance: bool,
        dest_supervisor_balance: bool,
        deferred: Option<(Memo, u64)>,
    },
    /// Schedule a core's JAM `assign`. A queue violating either length rule
    /// below aborts Refine with `Err(RefineLog::InvalidAuthorizerQueue)`.
    /// See §5.1 and §7.1. **Coretime chain only.**
    AssignCore {
        /// Must be below `C_corecount`, whether or not the core is currently
        /// active. Otherwise Refine fails with `Err(RefineLog::InvalidCoreIndex)`.
        core: CoreIndex,
        /// Between 1 and `AUTH_QUEUE_SIZE` authorizer hashes.
        queue: Vec<AuthorizerHash>,
        /// `None` keeps this service as the core's assigner. `Some(s)` hands the
        /// core to `s` and requires exactly `AUTH_QUEUE_SIZE` hashes. See §7.1.
        new_assigner: Option<ServiceId>,
        /// Timeslot at which the queue should be applied.
        jam_slot: Timeslot,
    },
    /// Append a chunk of upcoming validator keys to `staged_validator_keys`.
    /// `keys` holds at most 30 keys, and a longer chunk aborts Refine with
    /// `Err(RefineLog::TooManyValidatorKeys)`. May be sent at most once per
    /// Refine invocation, and a repeat aborts Refine with
    /// `Err(RefineLog::SetValidatorKeysRepeated)`. See §5.3.
    /// **Asset Hub only.**
    SetValidatorKeys { keys: Vec<ValidatorKey>, is_last: bool },
    /// Remove every `incoming_transfers` bucket up to and including this bucket
    /// id. See §5.1. **Asset Hub only.**
    CleanUpBucketsUpTo(BucketId),
    /// Replace the Parachain Service's own service code. See §5.4.
    /// **Asset Hub only.**
    UpgradeService { code_hash: Hash, len: Compact<u32>, min_acc_gas: u64, min_memo_gas: u64 },
    /// Upsert a parachain's head data. See §6.3. **Coretime chain only.**
    ParachainSetHead { para_id: ParaId, new_head: HeadData },
    /// Upsert a parachain's validation code. See §6.3. **Coretime chain only.**
    ParachainSetValidationCode { para_id: ParaId, new_validation_code: ValidationCodeRef },
    /// Remove all per-parachain state. See §6.4. **Coretime chain only.**
    ParachainCleanUp(ParaId),
    /// Set a parachain's `total_state_balance`. See §6.1. **Coretime chain only.**
    ParachainSetStateBalance { para_id: ParaId, new_total: Compact<Balance> },
}
```

Both variants reach JAM as a Gray Paper `WorkExecResult::Ok`, whose result blob is the
encoded digest. The combined size of all result blobs plus the authorizer trace in a
work-report is limited to **48 KiB** by the Gray Paper.

- **`Ok`** is returned when validation succeeds. The upward messages emitted during Refine
  (code upgrades, transfers, authorizer updates, etc.) are carried in this digest and
  applied by Accumulate.

- **`Err`** is returned when Refine fails (see `RefineLog`). If the digest passes the
  checks in §5.1, Accumulate records the failure in the parachain's `parachain_log` as a
  `LogEntry::Refine`, together with the work-report's authorizer trace. The trace comes
  from the authorizer and can identify who submitted the work package, such as the
  collator key in §7.1. A parachain can use it, for example, to slash a collator that
  claimed a slot it was not entitled to.

> **JAM `WorkErrorCode` is skipped.** When JAM substitutes a work-item with
> a Gray Paper `WorkExecResult::Error(WorkErrorCode)`, the Parachain
> Service's refine wrapper never produces a `ParachainWorkDigest`.
> Accumulate skips that work-item as if it did not exist, with no
> `parachain_log` entry and no state change.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

Refine is invoked **per work item** by JAM. For each work item at
index `item_index` the Parachain Service performs:

1. Reads the authorizer config via `fetch` and decodes the `authorized_paras`
   prefix (§3.2). If the config is not prefixed with a `Vec<ParaId>`, Refine panics (§3.3)
   instead of logging, because there is no authoritative `para_id` to attribute an entry to.
2. Takes `para_id = authorized_paras[item_index]` as authoritative for this item.
3. Decodes the `ParachainCandidate` from the work item payload. If the payload fails to
   decode, Refine panics (§3.3).
4. Fetches the validation code via `historical_lookup` (using `validation_code`).
   If the lookup returns `None` (the preimage isn't available in the service's
   store at the lookup-anchor), aborts with `Err(RefineLog::ValidationCodeLookupFailed)`.
5. Instantiates a child PVM with the validation code.
6. Executes the validation code (the `jam_validate_block` call).
7. Assembles a `ParachainWorkDigest` from the validation code's host-function side effects and the
   authoritative `para_id` (see §4.2).
8. Checks that the encoded digest (head data + upward messages) plus the
   work-report's authorizer trace fits in the Gray Paper's 48 KiB
   combined-result-blob budget. If not, aborts with
   `Err(RefineLog::RefineOutputTooLarge)`. Parachain-driven overflow (upward
   messages exceeding the 40 KiB budget) aborts earlier with
   `Err(RefineLog::UpwardMessagesTooLarge)` inside `send_upward_message`.

Any error in Refine aborts it immediately with that error.

Because Refine is stateless, it cannot write to service storage.

### 4.2 Validation Code Entry Point

The Parachain Service's Refine spawns a child PVM and calls the validation code's single entry point:

```rust
fn jam_validate_block() -> ()
```

The validation code reads its inputs (PoV, context, downward transfers) and writes its outputs
(head data, code upgrades, transfers) through host functions. It returns nothing. The
Parachain Service's Refine wrapper assembles the `ParachainWorkDigest` from the accumulated
host-function side effects.

Validation code has two ways to fail, and they differ in what is recorded. Calling
`report_error(data)` aborts it immediately and fails Refine with `RefineLog::Opaque(data)`.
Any other abnormal exit (panic, trap, failed execution) is not caught. The service's entire
`refine` fails with it, so the work-digest's result is a Gray Paper work error
(`WorkExecResult::Error`) and §3.3 applies. Recording a failure is therefore opt-in, and
validation code that wants a failure to leave no trace panics.

The Refine wrapper also fails the invocation as `Err` unless the validation code called both
`set_parent_head_hash` and `set_head` exactly once.

### 4.3 Host Functions & PVM Imports

On JAM, validation code executes inside a child PVM instance spawned by the Parachain Service's Refine
function. The child PVM's heap is capped at **1 GiB**, the upper bound `grow_heap` can reach.
**Hashing** and **signature verification** run as PVM guest code, not as host calls. Their
performance impact is small, and future PVM improvements should shrink it further
(see the [benchmarking findings](https://github.com/paritytech/parachain-service/issues/13)).

Every host function is imported at a **fixed index**. Those forwarding a JAM host call keep
its Gray Paper index. Those native to the Parachain Service are numbered from 200 up.

#### JAM host functions

Forwarded unchanged. Signatures and operands are specified in the Gray Paper and are
not restated here:

| Index | Host function | Purpose |
|---|---|---|
| 0 | `gas` | The remaining gas budget. |
| 1 | `grow_heap` | Expand the RW data region. |
| 2 | `fetch` | Read the work package and its context: the package itself, the refine context, the authorizer config and token, the work-item summaries, payloads and extrinsics, and the import segments. |
| 7 | `historical_lookup` | Read a service's preimage store at the lookup-anchor, for both own and foreign lookups. |
| 8 | `export` | Write a segment to the JAM Data Lake, e.g. an outbound XCMP payload. |

#### Parachain Service host functions

Native to the service. Their effects are carried in the work digest and applied by
Accumulate:

| Index | Host function | Returns | Purpose |
|---|---|---|---|
| 200 | `set_parent_head_hash(hash: Hash)` | `()` | Declare the parent head hash this candidate was built on, as the hash of the parent `head_data`. **Mandatory**: every Refine invocation must call this exactly once or the invocation is invalid (treated as `Err`). The hash is forwarded to Accumulate, which checks it against the para's current head (§5.1 step 4). |
| 201 | `set_head(new_head: HeadData)` | `()` | Declare the new head data this parachain block produced. **Mandatory**: every Refine invocation must call this exactly once or the invocation is invalid (treated as `Err`). Aborts Refine with `Err(RefineLog::HeadDataTooLarge)` if `new_head` exceeds the 4 KiB `HeadData` bound. The head data is forwarded to Accumulate as `ParachainWorkDigest.head_data` and written into `ParaInfo.head_data` on enactment (§5.1 step 5). Distinct from the Coretime-only `ParachainSetHead`, which forcibly overwrites *another* para's head outside the normal block lifecycle (§6). |
| 202 | `send_upward_message(msg: UpwardMessage)` | `()` | Append one upward message to `ParachainWorkDigest.upward_messages`. Aborts Refine with `Err(RefineLog::UpwardMessagesTooLarge)` if the message would carry the encoded upward messages past the parachain's fixed **40 KiB** budget. Individual variants carry further requirements, documented on the variant. Panics if `msg` fails to decode. |
| 203 | `report_error(data: BoundedVec<u8, 1024>)` | `!` | Abort the validation code, failing Refine with `RefineLog::Opaque(data)`. Any bytes beyond 1024 are truncated. Never returns. This is the only way validation code records a reason for its failure. See §4.2. |

`UpwardMessage` is part of the parachain-visible ABI. Its SCALE encoding is
stable, so a message's `encoded_size()` is computable inside the validation code. The 40 KiB
budget counts the encoded messages alone.

Variants marked **Asset Hub only** or **Coretime chain only** are accepted from
that parachain alone. A variant carrying a `para_id` is further restricted to the
calling parachain, except from the Coretime chain, which may name any parachain
(§6.4). Violating either rule aborts Refine with
`Err(RefineLog::RestrictedHostFunction)`.

A single Refine invocation may emit at most `MAX_UPWARD_MESSAGES = 1024` upward
messages. If the validation code exceeds this, the invocation fails with
`Err(RefineLog::TooManyUpwardMessages)`.
This bounds the number of side effects Accumulate must replay per work item,
independently of the 48 KiB combined-result-blob budget.

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage.

Accumulate for the Parachain Service covers the parachain-specific parts of what the
relay chain's `enact_candidate` does today. JAM handles availability, approvals, and disputes
natively (see §2). The work runs in three phases, in order: due authorizer-queue flushes,
incoming-transfer processing, and per-work-package work. The first two form the
always-accumulate work. Because always-accumulate runs *before* the work packages, a queue
scheduled by a work package is normally applied in a later block's always-accumulate, once
its `jam_slot` arrives. A queue whose `jam_slot` is already due (`jam_slot <= now`) when the
scheduling message is processed is instead applied inline right away, since
always-accumulate has already run.

The always-accumulate phases must stay within the always-accumulate allowance, since every
report's gas is budgeted for that report alone (see the gas gate under *Per-work-package
work* below). Their cost must therefore be bounded and **benchmarked**, so that a block
heavy in due assigns or incoming transfers cannot eat into report gas.

#### Apply due assigns (before work packages)

For each core in `pending_assign_cores` whose due timeslot has been reached, call JAM
`assign(core, queue, assigner)` from its `pending_assigns` entry: the cached queue filled
to 80 slots (§7.1), and the cached `assigner`, or this service's own id if none is set.
Due timeslots are kept only in `pending_assign_cores`, so Accumulate reads a core's
`pending_assigns` entry only when that core is due.

- If the call succeeds, the core is dropped from both maps. The exception is a core that
  stays assigned to this service and whose queue needs rewriting every 80 blocks (§7.1). It
  is re-armed 80 blocks out with its rotation advanced.
- If JAM rejects it because this service is no longer the core's assigner, the core is
  dropped from both maps and `AccumulateLog::CoreNotAssignable` is recorded in the
  Coretime chain's log.

#### Incoming transfer processing

JAM credits a transfer's balance to the destination service unconditionally, before the
service's code runs and even if that code panics or runs out of gas. The service
therefore **cannot refuse or fail an incoming transfer**. Its only decision is whether to
*record* it in `incoming_transfers` for Asset Hub to act on. Recording is **best effort**,
and the funds are kept either way.

`MAX_INCOMING_TRANSFERS` is the portion of the queue Asset Hub pre-provisions in its
baseline (§6.1), not a hard cap. While the queue holds fewer than that many transfers, a
new one is recorded unconditionally, since the storage it occupies is already paid for.
Once the queue holds `MAX_INCOMING_TRANSFERS`, every further transfer is unprovisioned
and is recorded only if its `amount` covers its own entry's cost (the per-bucket figure
derived in §6.1). One that does not is dropped, with no record and no log entry.
Admitting one raises Asset Hub's `used_state_balance` and `total_state_balance` alike by
that entry cost, and clean-up lowers both by the same, so its available state balance is
unchanged whatever the queue holds.

Recording appends to the bucket the current accumulate invocation opened. A bucket is
closed once it holds `MAX_TRANSFERS_PER_BUCKET` transfers or the invocation that opened
it ends. The next arrival opens `last_bucket + 1`, or `0` when the queue is empty. Ids
are thus contiguous, so Asset Hub can enumerate the queue from the two endpoints alone.
The per-bucket cap bounds the cost of reading any one bucket.

`CleanUpBucketsUpTo(bucket_id)` removes whole buckets from `first_bucket` up to and
including `bucket_id` and points `first_bucket` at the first survivor. Once nothing
remains, the `incoming_transfer_buckets` entry is removed, so ids restart from `0` rather
than increasing forever.

This is safe as long as the JAM block Asset Hub references only moves forward. Asset Hub
can then only name buckets it has seen, so nothing it has not read is removed.

**`min_memo_gas` must be benchmarked** against the real cost of admitting one transfer,
and `MAX_INCOMING_TRANSFERS` derived from it.

#### Per-work-package work

Performed once for each work package accumulated in this block, in order. A Gray Paper
`WorkExecResult::Error` result, such as a panic in `refine`, is skipped. It records no
`parachain_log` entry, changes no state, and never reaches the steps below.

**Gas gate.** JAM funds the invocation from the gas limits declared by the reports passed
to it, plus the always-accumulate allowance on the first accumulation round (§3). The
service still budgets **per report**. Each report is checked against its own declared
limit, not against what is left in the pool, so no parachain can spend gas that another
one registered. Running out of gas mid-invocation discards everything not yet
checkpointed, and JAM does not retry the lost reports. The service therefore never
processes a report it cannot pay for in full. Before any of the steps below run for a report:

- **Base cost**: the fixed cost of applying any work report, independent of its
  contents. This must be **benchmarked**.
- **Report cost**: derived from the report's contents before any of it is applied. It
  covers the upward messages to be replayed (§4.3) and the state writes each implies.
  These costs must be **benchmarked**.
- **Fit check**: if base plus report cost exceeds the report's own gas limit, the report
  is skipped and the next one is checked.

A report that clears the gate runs the steps below, and the invocation `checkpoint`s once
they are done, so a later report running the budget dry cannot undo it.

A candidate **rejected** at any step below has no effect. No later step runs for it, and it
writes no state, records no log entry, and prunes nothing. The steps are:

1. **Registration check**: Reject the work package, without a `parachain_log` entry, if
   `para_id` is not in `parachains` or its `ParaInfo` has `is_deregistering == true`
   (§6.4). A deregistering para is treated as if it no longer exists.
2. **Validation code check**: This is the authoritative check. Reject the work package if
   the digest's `validation_code` is not the hash of `ParaInfo.validation_code`.
3. **Refine-result dispatch**: A **Refine failure** (`ParachainWorkDigest::Err`, see §3.3)
   is appended to `parachain_log[para_id]` as a `RefineLogEntry` carrying its `RefineLog`
   and the work-report's authorizer trace, under the eviction rules below. Processing then
   stops, with no further steps and no log pruning. A **Refine success**
   (`ParachainWorkDigest::Ok`) proceeds through the remaining steps.
4. **Parent head check**: Verify the work digest's `parent_head_hash` equals
   `hash(ParaInfo[para_id].head_data)`. If not, the candidate is rejected. This prevents
   a collator from including a candidate that was built on top of a stale, skipped, or
   non-canonical parent head.
5. **Head data update**: Write the new `head_data` from the work digest into
   `ParaInfo` for the parachain.
6. **Process host-function calls from Refine**: Replay the `UpwardMessage`s in the work
   digest, applying each one's effect (code upgrades, transfers, authorizer queue
   updates, validator key updates, etc.). See the `UpwardMessage` variants in §3.3 for
   the full list. Refine already rejects messages the parachain was not entitled to
   emit, so any such message reaching Accumulate is dropped without a log entry. The
   replay may emit `AccumulateLog` events.

All `AccumulateLog` events emitted by the step 6 replay are collected and appended to
`parachain_log[para_id]` as a single `LogEntry::Accumulate`, where `para_id` is the
parachain that submitted the work package. Every append to `parachain_log[para_id]`,
whether the `RefineLogEntry` from step 3 or this `LogEntry::Accumulate`, is subject to the
eviction rules below.

**Log pruning and eviction.** Accumulate prunes each parachain's `parachain_log` and caps
its size. When a candidate is **accepted**, entries whose inline timeslot is strictly less
than its lookup-anchor timeslot are pruned before any of that candidate's own effects are
applied. Only accepted candidates prune. The anchor is chosen by whoever submitted the
package and pruning ignores rank, so if rejected candidates could prune, anyone holding
coretime could erase a parachain's log wholesale and bypass the ranking below. The cap is
64 KiB of total encoded size rather than a fixed number of entries, so an entry takes up
only the space it needs.

When a new entry would push the log over 64 KiB, eviction follows a **fixed rank
order**, lowest rank discarded first:

| Rank | Entry |
|---|---|
| 0 | `RefineLogEntry` whose error is **not** `Opaque` |
| 1 | `RefineLogEntry` carrying `Opaque` |
| 2 | `LogEntry::Accumulate` |

The log is a `Vec` built by appending, so entries sit in arrival order and their inline
timeslots are non-decreasing. Eviction picks the lowest occupied rank *at or below* the
incoming entry's own rank and, within that rank, the entry with the earliest inline
timeslot. This repeats until the log fits. Entries sharing a rank and a timeslot are
equally old, so it does not matter which of them is evicted.

A new `Opaque` therefore displaces rank-0 entries first and, once none are left, the oldest
existing `Opaque`. A new accumulate entry displaces refine entries of either rank before
the oldest accumulate entry. An entry is **never** evicted to make room for something of
lower rank. When only higher-ranked entries remain, the incoming entry is dropped instead.

**Why the ranking exists.** Anyone can buy coretime on a core assigned to a parachain. A
work package submitted that way still reaches Refine, so its failures are recorded against
the parachain even though the parachain did not cause them. All such failures land in
rank 0. A buyer can churn rank 0 against itself, but can never evict the parachain's own
reports or its on-chain state changes. The damage is limited to losing diagnostics that
were the attacker's own noise.

**What this means for parachain implementors.** `parachain_log` is the only channel
through which a parachain learns why its candidates failed, and it is lossy. Entries below
a candidate's lookup-anchor are pruned, and entries are evicted under capacity pressure.
Parachains should read it promptly through the validation inputs (§5.4 phase 2 shows the
pattern) and never treat it as a durable record. Rank-0 entries can be both evicted and
produced by anyone holding coretime, so parachain logic must depend neither on their
presence nor on their absence. Anything a parachain needs to act on reliably belongs in an
`Opaque` payload its own validation code emitted, or in an accumulate event, which records
a state change that has already happened.

#### Outgoing transfers

Replaying a `TransferOut` (step 6) forwards it to JAM `transfer`. `deferred`
selects between the two modes that host-call offers (Gray Paper, `transfer`):

| | `deferred = None` (plain move) | `deferred = Some((memo, gas))` |
|---|---|---|
| Destination code | none runs | destination's Accumulate runs with `gas` |
| Gas charged | `C_gasT` only | `C_gasT + gas` |
| Balance credited | immediately | when the destination accumulates |
| Requires supervision of `dest` | **yes** | no |

`gas` comes out of the Parachain Service's shared Accumulate pool (the gas gate under
*Per-work-package work* above), so it is charged against the limit the requesting
candidate registered.

`source` names the debited account, `None` meaning the Parachain Service itself.
`source_supervisor_balance` and `dest_supervisor_balance` pick which balance is used
on each side: the supervisor balance when true, the regular balance when false.

### 5.2 Parachain Code Upgrade Lifecycle

A parachain switches to new validation code in two explicit steps, both carried by
`RequestCodeUpgrade` (§3.3): an **`Announcement`** naming the code, and a later
**`Apply`** that makes it active. Neither step puts the code into the preimage store. The
parachain solicits it beforehand with `Solicit` (§3.3).

The split settles availability before the switch. Code must be available to be announced,
so an `Apply` can never leave the parachain pointing at validation code JAM cannot serve.

```
Step 1: Solicit the code
    Parachain emits Solicit { target: Parachain(self), hash, len }, charged to its
    state balance (§6.1). Anyone (collator, block author, third party) then submits
    the PVM blob to JAM, which validates it against the solicitation.
    │
    ▼
Step 2: Announcement
    Parachain emits RequestCodeUpgrade { hash, len, phase: Announcement }.

    Both must hold:
      - the parachain references (hash, len)
      - query(hash, len) reports it available at the work report's lookup
        anchor

    either fails    -> rejected with CodeUpgradeNotAvailable, nothing changes
    already active  -> no-op
    otherwise       -> announced_upgrade = (hash, len), replacing any
                       standing announcement

    The active validation_code is untouched. Candidates must still be validated
    with it (§5.1 step 2) until the Apply lands.
    │
    ▼
Step 3: Apply
    Parachain emits RequestCodeUpgrade { hash, len, phase: Apply } at a point
    of its choosing.

    not the standing announcement, or nothing announced
                    -> rejected with CodeUpgradeNotAnnounced, nothing changes
    otherwise       -> validation_code = announced code, and
                       announced_upgrade is cleared

    The switch is immediate. The candidate carrying the Apply was itself
    validated with the old code, and every candidate after it must use the new
    one.
```

### 5.3 Validator-Key Updates

A full `stagingset` (Gray Paper, Safrole section, validator-key definitions) is up to
`1023 × 336 B ≈ 336 KiB`. That is too large for a single work-report's
`C_maxreportvarsize = 48 KiB` result-blob budget, and JAM's `designate` accepts only the
complete vector. The Parachain Service therefore buffers chunks in `staged_validator_keys`
across multiple Asset Hub blocks until Asset Hub signals completion via `is_last`. Each
block carries one chunk, since `SetValidatorKeys` may be sent at most once per Refine
(§4.3).

When Accumulate replays a `SetValidatorKeys { keys, is_last }` upward message it:

1. If `is_last == false`, appends `keys` to `staged_validator_keys`. The staging buffer
   is reserved at its worst case in Asset Hub's baseline footprint (§6.1). An append
   that would grow it beyond that capacity (the 1023-key bound on
   `staged_validator_keys`) is rejected with `AccumulateLog::StagedValidatorKeysOverflow`,
   leaving the buffer unchanged.
2. If `is_last == true`, clears the buffer and calls JAM `designate` with the
   assembled set (prior buffer + `keys`). `designate` accepts it only if its
   length is in `valcount`: **a multiple of 3, at least 6 and at most 1023**.
   Otherwise JAM's `stagingset` is left unchanged and
   `AccumulateLog::DesignateRejected` is recorded against the Asset Hub `ParaId`.
   An **empty** `keys` aborts instead: the buffer is discarded and `designate` is
   not called.

A worst-case 1023-key rotation takes ~35 Asset Hub work packages (≈ 3.5
minutes at 6 s timeslots). State-balance accounting for the staging
buffer is covered in §6.1.

### 5.4 Service Self-Upgrade

**Asset Hub** controls upgrades of the Parachain Service's own code. Asset Hub
triggers the upgrade by emitting `UpwardMessage::UpgradeService` (§3.3), which the
Refine wrapper rejects from any other parachain. Accumulate forwards it to JAM's `upgrade`
host call after verifying that **Asset Hub references** the new code's preimage (§6.1)
and that the preimage is **available for lookup**, meaning JAM's `query` reports it as
provided or re-requested. Asset Hub cannot `Forget` its reference to the active code.

```
Phase 1: Solicit
    Asset Hub emits Solicit { target: Parachain(asset_hub_para_id), hash: new_code_hash, len }
    (§6.1).

Phase 2: Verify Solicit
    Asset Hub waits for its next block to be built on top of a JAM
    block whose state reflects the accumulated solicit, then reads
    its parachain_log via the validation inputs and confirms no
    AccumulateLog::InsufficientStateBalance for new_code_hash. If
    insufficient, Asset Hub aborts.

Phase 3: Upgrade
    Asset Hub emits UpgradeService { code_hash: new_code_hash, .. }.
    Accumulate forwards to JAM upgrade if Asset Hub references the preimage
    and it is available. Otherwise it logs
    AccumulateLog::ServiceUpgradePreimageMissing.

Phase 4: Activate
    On the next JAM invocation the Parachain Service runs under the
    new code.

Phase 5: Forget
    Asset Hub observes the new codehash in Parachain Service state
    and emits Forget { target: Parachain(asset_hub_para_id), hash: old_code_hash, len } (§6.1).
```

### 5.5 Parachain Head Commitment

`accumulate` may return a 32-byte hash. The Parachain Service uses it to commit to
**parachain heads**. An accumulate invocation that changed at least one head builds a
binary Merkle tree over the heads it changed and returns its root. One that changed
none returns nothing and adds no entry to the accumulation output log.

```rust
enum MerkleTree {
    Node(Hash, Hash),
    Leaf { para_id: ParaId, head_hash: Hash },
}
```

- A leaf's `head_hash` is `keccak_256` of the parachain's `head_data`.
- Every element's hash is `keccak_256` (as specified by Ethereum) of its SCALE encoding.
  The variant discriminant is therefore covered by the hash, so a leaf hash can never
  collide with a node hash. A `Leaf` encodes to 37 octets (discriminant, 4-octet
  `para_id`, 32-octet `head_hash`) and a `Node` to 65 (discriminant, two hashes).
- One leaf per parachain whose `head_data` changed during the invocation, carrying the
  value it holds when the invocation ends. A parachain written more than once within
  the invocation, by a candidate and then a forced `ParachainSetHead`, still
  contributes exactly one leaf.
- Leaves are ordered by ascending `para_id`, so every verifier builds the same tree and
  can locate a parachain's leaf without extra data.
- With exactly one changed head the root is that leaf's hash.

**A block may carry more than one root.** JAM can invoke `accumulate` of the same
service several times in one block. Each invocation yields its
own root, and the accumulation output log records all of them.

**A root proves only what changed.** The absence of a leaf means a parachain's head did
not change in that invocation, not that it holds any particular value. Proving a
parachain's current head therefore means locating the most recent root whose tree
carries a leaf for it, and proving against that.

---

## 6. Parachain Management

Parachain lifecycle and management is driven by the **Coretime chain**, which owns the
policy layer: ParaId allocation, deposits, and deciding when to create, overwrite, or
clean up a parachain's state.

The Parachain Service accepts four low-level, idempotent upward messages (§3.3) that drive
state-balance management, registration, forced updates, and deregistration:

- `ParachainSetStateBalance { para_id, new_total }`: set the parachain's quota
- `ParachainSetHead { para_id, new_head }`: upsert head data
- `ParachainSetValidationCode { para_id, new_validation_code }`: upsert validation code
- `ParachainCleanUp(para_id)`: remove all per-parachain state

All four are Coretime-chain-only. The Parachain Service performs no rights-checking of its
own and **does not enforce ParaId uniqueness**. The Coretime chain is the sole authority on
which `ParaId`s are live and who owns them. `ParachainSetStateBalance` is the sole creator
of `ParaInfo` (see §6.1). `ParachainSetHead`, `ParachainSetValidationCode`, and
`ParachainCleanUp` silently no-op on a `ParaId` whose `ParaInfo` doesn't exist yet, so
Coretime must emit `ParachainSetStateBalance` first in any registration sequence. On an
existing `ParaId`, `ParachainSetHead` / `ParachainSetValidationCode` overwrite the current
value, which is what forced recovery uses (§6.3).

### 6.1 State-Balance Accounting

JAM bills each service for **everything it holds in state** (its storage key/value
entries, its solicited preimages, and the protocol-level service record itself) by
requiring the service to keep a minimum balance proportional to that footprint. The
Parachain Service inherits that obligation for the *aggregate* footprint of all
parachains it hosts, and re-attributes it **per parachain** via `used_state_balance`.

The per-parachain footprint includes everything the service stores under this
parachain's `ParaId`: `ParaInfo`, solicited preimages, the `parachain_log` reserve,
slots in shared structures like `preimage_registry`, and any future per-`ParaId`
state.

Each parachain is billed as if it were the **sole user** of every data structure it
touches in service state: its `used_state_balance` is the sum of each structure's
footprint computed as though the stored value held only this parachain's contribution.

JAM's threshold balance (Gray Paper, *Account Footprint and Threshold Balance*)
charges per *item* as well as per *octet* (`C_itemdeposit = 10`, `C_bytedeposit = 1`),
so the two units of state this service holds cost:

| State | Items | Cost |
|---|---|---|
| solicited preimage of length `z` | 2 | `101 + z` |
| general-storage entry | 1 | `44 + \|value\| + \|key\|` |

Footprints are therefore **balance units**, not bytes. JAM's flat `C_basedeposit` is
per-service and never part of a per-parachain footprint.

Shared structures like `preimage_registry` end up over-collateralized, since every
referencer pays for a full entry, but existing parachains' contributions never need
recomputing when the referencer set changes.

#### Total balance management (Coretime chain only)

The Coretime chain is the sole authority on `total_state_balance`. It calls
`ParachainSetStateBalance { para_id, new_total }` to set the value: at registration
to create the initial budget (see §6.2), and later to raise it when the parachain needs
more state or to lower it to reclaim balance the parachain does not use.

`ParachainSetStateBalance` is the sole creator of `ParaInfo`. Called on a
previously-unused `ParaId`, it creates a fresh entry with
`total_state_balance = new_total`, `used_state_balance = baseline_footprint`, and
the other fields uninitialized (to be filled in by subsequent `ParachainSetHead` /
`ParachainSetValidationCode` calls in the same registration sequence). Called on
an existing `ParaId`, it overwrites `total_state_balance` in place.

In either case the call is applied only if `new_total >= used_state_balance`, so
`total_state_balance` always covers the parachain's state. Otherwise nothing changes, and an
`AccumulateLog::StateBalanceUpdateRejected { para_id, attempted, reason }` with reason
`BelowUsed { current_total, current_used }` is appended to the Coretime chain's
`parachain_log` (§5.1) so it can observe the rejection and size a retry. A deregistering
`ParaId` (§6.4) is rejected the same way with reason `ParachainIsDeregistering`. To free
state balance, `used_state_balance` must first be reduced by releasing state via
`Forget` / `RemoveKV`, emitted either by the parachain itself or by the Coretime chain on
its behalf (see §6.4).

The Coretime chain verifies that the user can cover at least the baseline before starting
the registration sequence.

Deposits, sizing, and refunds are owned end-to-end by the Coretime chain. End users
interact with it through its usual extrinsics, and the Coretime chain reflects the results
into the Parachain Service via `ParachainSetStateBalance`.

#### Preimage handling

JAM allows only one `(hash, len)` solicitation per service. The Parachain Service is a
single service hosting many parachains, so they share one request via `preimage_registry`,
where each entry records the set of `ParaId`s referencing the hash. JAM `solicit` is called
when the set goes from empty to non-empty, and JAM `forget` when it becomes empty again.

A `Forget` of code still in use is rejected with `AccumulateLog::CanNotRemoveCode`. Code
counts as in use while it is the target's `validation_code` or `announced_upgrade` (§5.2)
or, for Asset Hub, the Parachain Service's active code (§5.4).

A `Forget` that leaves other referencers drops the parachain from `referencers` and
refunds its footprint right away. Removing the last referencer of an *available* preimage
takes **two steps**. A JAM `forget` does not delete it but marks the request unavailable. Only a **second** `forget`, no earlier than
`C_expungeperiod = 19 200` timeslots (~32 h) later, expunges it. The service keeps no
bookkeeping for this. When a `forget` removes the last referencer without expunging the
preimage, Accumulate appends an `AccumulateLog::ForgetAgainAt { hash, len, due }`, where
`due = now + C_expungeperiod`, to the log of the parachain that emitted the `Forget`
(§5.1). The last referencer stays in `referencers` and is still charged the full
footprint. That parachain emits `Forget { target: Parachain(para_id), hash, len }` again
once the timeslot is *strictly after* `due` to complete the expunge and free the
footprint.

A preimage that was solicited but **never provided** to JAM is different. A single
`forget` of its last referencer drops the request outright. There is nothing to expunge,
so the footprint is freed immediately and no `ForgetAgainAt` is logged.

**Rescue.** During the ~32 h window between the two forgets, JAM still holds the blob, so
a `solicit` can make the request available again. The service does this automatically. If
a parachain references an entry whose last referencer is awaiting expunge, Accumulate
re-forwards JAM `solicit` and the preimage serves lookups again. The rescuing parachain
becomes the entry's sole referencer. The parachain awaiting expunge is dropped and
refunded, since it had already forgotten the preimage and was only kept as a stand-in for
the pending second forget. It no longer references the preimage, so its second `Forget` is
a no-op without a log entry. To the upstream parachain this looks like a normal successful
forget.

A rescue does **not** reset the expunge deadline, so the last referencer may need three
`Forget`s to expunge a rescued preimage:

1. A `Forget` before the original expunge period has ended changes nothing. The parachain
   gets a `ForgetAgainAt` whose `due` is the end of that period.
2. A `Forget` after that `due` makes the preimage unavailable again. The parachain gets a
   `ForgetAgainAt` whose `due` is the end of a new expunge period.
3. A `Forget` after the new `due` expunges the preimage and frees the footprint.

If the first `Forget` comes after the original expunge period has ended, step 1 is
skipped.

Applying the sole-user rule, a single referencer's **preimage footprint** is the
sum of two JAM entries: the **preimage request** (`101 + len`) and its
**`preimage_registry` entry** at `44 + |value| + |key|`, with `|value| = 5` (a singleton
`{ParaId}` referencer set) and `|key| = 37` (1 B map tag + 32 B hash + 4 B len), giving
`86`. That is **187 + len** per referencer, even though the on-chain entry may hold
many referencers.

#### Sizing the baseline footprint

`baseline_footprint` is the worst-case state cost of an empty parachain: the
`(ParaId, ParaInfo)` entry plus the `(ParaId, parachain_log[para_id])` entry, with
every bounded field SCALE-encoded at its maximum so the value is static across the
parachain's lifetime. Each is one general-storage entry. Taking `ParaId = u32` (4 B),
`Hash = 32 B`, `Timeslot = u32` (4 B), and `Balance = u64`, so
that `Compact<Balance>` is sized at its worst case of 9 B:

`(ParaId, ParaInfo)` entry:

```
JAM per-entry octet overhead                                       =      34
map tag                                                            =       1
ParaId (key)                                                       =       4
head_data: BoundedVec<u8, 4096> = 2 (compact len) + 4096           =   4 098
validation_code: Option<ValidationCodeRef> = 1 + 32 + 4            =      37
announced_upgrade: Option<ValidationCodeRef> = 1 + 32 + 4          =      37
total_state_balance: Compact<Balance>                              =       9
used_state_balance: Compact<Balance>                               =       9
is_deregistering: bool                                             =       1
                                                          octets       4 230
                                                          1 item          10
                                                                     -------
                                                                       4 240
```

`(ParaId, parachain_log[para_id])` entry. The log value is bounded by its exact encoded
size, with entries sized by their actual SCALE length rather than their worst case. The
64 KiB cap covers every log element plus the vector's own length prefix. JAM's 34 B
per-entry overhead and the 5 B storage key (1 B map tag + 4 B ParaId) sit on top, so the
service reserves a flat 64 KiB + 34 + 5 regardless of current contents:

```
JAM per-entry octet overhead                                       =      34
storage key (1 B map tag + 4 B ParaId)                             =       5
parachain_log value (flat cap): 64 KiB                             =  65 536
                                                          octets      65 575
                                                          1 item          10
                                                                     -------
                                                                      65 585
```

**`baseline_footprint = 4 240 + 65 585 = 69 825`** balance units per parachain.

#### Asset Hub baseline footprint

Asset Hub additionally owns the service-global state items as privileged caller. Its
`total_state_balance`, provisioned at genesis, must cover them. Each is billed as a
general-storage entry, so a `Map` costs one entry per key it holds while a `BoundedVec` or
a singleton costs one entry in total.

Of these only `incoming_transfers` grows with the transfer bound. Taking
`CoreCount = 341`, `AuthorizerHash = 32 B`, `ServiceId = 4 B`, `Memo = 128 B`,
`CoreIndex = 2 B`, authorizer-queue length = 80, and `Amount = u64`, so that
`Compact<Amount>` is sized at its worst case of 9 B, the fixed part is:

```
staged_validator_keys: BoundedVec<ValidatorKey, 1023>  · 1 item
  34 + 1 (key) + 2 + 1023 × 336                            octets    343 765
pending_assigns: Map<CoreIndex, PendingAssign>  · 341 items
  341 × (34 + 3 (key) + 2 + 80 × 32 + 5 (Option<ServiceId>))  octets    887 964
pending_assign_cores: BoundedVec<(CoreIndex, Timeslot), 341>  · 1 item
  34 + 1 (key) + 2 + 341 × (2 + 4)                         octets      2 083
incoming_transfer_buckets: IncomingTransferBuckets  · 1 item
  34 + 1 (key) + 8 + 8 + 4 (count)                         octets         55
                                                  octets subtotal   1 233 867
                                                    344 items × 10      3 440
                                                                    ---------
                                                                    1 237 307
```

Writing `N` for `MAX_INCOMING_TRANSFERS`, the queue's worst case is **maximal
fragmentation**: every transfer alone in its own bucket, as produced by one transfer per
accumulate invocation. `MAX_TRANSFERS_PER_BUCKET` does not improve this, since it limits
how much a single bucket can hold, not how little. Every bucket holds at least one
transfer, so bounding the transfer count also bounds the bucket count.

```
incoming_transfers: Map<BucketId, IncomingTransfers>  (worst case N items)
  N × (34 + 9 (key) + 1 + 142 (transfer))                       186 × N
  N storage items × 10                                           10 × N
                                                              ---------
                                                              196 × N
```

The whole reservation is therefore

```
asset_hub_global_items = 1 237 307 + 196 × N
```

`N` is provisional until `min_memo_gas` is benchmarked and the bound derived from it
(§5.1), and it is the only input that moves. Entries past `N` are not part of this
reservation. Each is charged to Asset Hub as it arrives and refunded as it drains
(§5.1). At `N = 1000` the reservation is `1 237 307 + 196 000 = 1 433 307`, or
**≈ 1.37 MiB**, on top of the generic per-para baseline.

#### Key-Value storage footprint

Each `(ParaId, key) -> value` entry in `key_value_storage` pays the sole-user
general-storage cost `44 + |value| + |storage_key|`. The value is stored exactly as the
parachain sent it, and the storage key is the map tag, the parachain id and the user key
as sent:

```
kv_entry_footprint(k, v) = 44
 + v (value, stored as sent)
 + 1 (map tag) + 4 (ParaId) (per §3.1 storage-key encoding)
 + k (user key, as sent)
 = 49 + k + v
```

A `SetKV` computes the change in `used_state_balance`: the new entry's footprint, or the
difference in value length, `new_v − old_v`, when overwriting an existing key. JAM `read`
returns the stored value's length, so calling it with an output length of 0 yields
`old_v` without copying the value. A positive change must fit within
`total_state_balance` before the write is applied. A negative change (an overwrite with a
smaller value) is credited back. A `RemoveKV` refunds `kv_entry_footprint(k, v)` for the
removed entry.

#### Write-time invariant

Every mutation that would grow `used_state_balance` is first checked against the headroom
left in `total_state_balance`. Without enough headroom the write is skipped and
`AccumulateLog::InsufficientStateBalance` is appended to the emitting parachain's log
(§5.1). Otherwise the write is applied and `used_state_balance` is raised atomically.
Baseline-covered state is pre-charged and needs no per-write check.

JAM's `write` returns `StorageFull` when the service's own balance cannot cover
the new footprint. Seeing it indicates a bookkeeping bug and can leave the entire
service stuck until manual intervention.

### 6.2 Registration

Registration is the composition of `ParachainSetStateBalance`,
`ParachainSetHead`, and `ParachainSetValidationCode` on a previously-unused
`ParaId`, in that order:

```
Coretime chain
    │  Account submits registration: genesis head + validation code hash + len.
    │  Coretime sizes the deposit per §6.1, allocates the ParaId, and emits:
    │      ParachainSetStateBalance { para_id, new_total: total }
    │      ParachainSetHead { para_id, new_head: genesis_head }
    │      ParachainSetValidationCode { para_id, new_validation_code }
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

`ParachainSetHead` and `ParachainSetValidationCode` also handle exceptional recovery, e.g.
unsticking a chain whose last included block cannot be built on, or swapping in new
validation code outside the normal upgrade lifecycle:

- `ParachainSetHead { para_id, new_head }` overwrites `ParaInfo.head_data`.
- `ParachainSetValidationCode { para_id, new_validation_code }` sets
  `ParaInfo.validation_code` to `Some(new_validation_code)`, solicits it, and clears any
  `announced_upgrade`. `used_state_balance` grows by its `preimage_footprint` to hold the
  new validation code, unless the parachain already references it, in which case the
  solicit is a no-op and nothing is charged. The displaced validation codes are
  left untouched, as on the normal upgrade path (§5.2). The call is rejected with
  `AccumulateLog::InsufficientStateBalance` if the new footprint wouldn't fit, so Coretime
  must raise `total_state_balance` first when needed.

```
Coretime chain
    │  Verifies the rights of the caller
    │  Emits ParachainSetStateBalance { para_id, new_total } if needed
    │  Emits ParachainSetHead { para_id, new_head } OR
    │        ParachainSetValidationCode { para_id, new_validation_code }
    ▼
Parachain Service (Accumulate)
    │  Applies the change, re-soliciting/forgetting preimages and adjusting
    │  used_state_balance as described above.
```

### 6.4 Clean-up (Deregistration)

```
Coretime chain
    │  Verifies the rights of the caller
    │  Emits ParachainCleanUp(para_id)
    ▼
Parachain Service (Accumulate)
    │  Rejects with TooMuchStateHeld if the parachain holds state beyond its
    │  baseline, its active validation code and its announced validation code.
    │  Otherwise forgets the active and the announced validation code:
    │
    │    all expunged  -> removes parachains[para_id] and parachain_log[para_id]
    │    otherwise     -> sets is_deregistering and stops until the retry
```

Requiring the parachain to drain its own extra state first keeps clean-up bounded. The
service only has to forget the two validation codes, never an unbounded set of solicited
preimages or KV entries. A parachain that can no longer produce
blocks cannot drain itself, so `Forget` and `RemoveKV` take a `para_id` (§3.3),
letting the Coretime chain free any parachain's state on its behalf.

The `TooMuchStateHeld` check allows `used_state_balance` up to `baseline_footprint` plus
the preimage footprints of `validation_code` and `announced_upgrade`, where set. A
clean-up that stops for a retry leaves both validation codes in place and charged until the
expunging `forget` succeeds, so the retry passes the same check.

While `is_deregistering` is set the service rejects every work package for the
parachain (§5.1). `Solicit`, `SetKV`, `RemoveKV`, `ParachainSetHead` and
`ParachainSetValidationCode` for it are no-ops, and `ParachainSetStateBalance` is
rejected with `ParachainIsDeregistering` (§6.1), so no new state accrues. Each
not-yet-expungeable validation code emits a `ForgetAgainAt { .., due }` into the
Coretime chain's `parachain_log` (§6.1), as does `TooMuchStateHeld` above. The Coretime
chain retries the call once the timeslot is strictly past the latest such `due`, and the
parachain is fully removed. This keeps all follow-up in a single message rather than
tracking per-preimage `forget` deadlines.

Coretime also handles deposit refund and any economic unwinding according to its
own policy.

---

## 7. Authorization & Coretime

Coretime on JAM is managed by the **Coretime chain** for **all** services, not just the
Parachain Service. The Coretime chain decides which service (and, for the Parachain
Service, which parachain) owns each core and therefore which authorizer queue should be
installed on it. JAM itself tracks core assignment and coretime usage as protocol state.

For the Parachain Service, the ownership boundary is:

- The **Coretime chain** decides which parachain owns each core and computes the desired
  authorizer queue for that core.
- The **Parachain Service** applies those decisions to JAM via the JAM `assign` host call,
  emitted as an `UpwardMessage::AssignCore` from the validation code or from the
  always-accumulate control path.
- JAM's `is_authorized` invocation then checks a work-package token against one of the
  authorizers currently in the core's authorizer pool.

### 7.1 Authorizer Design: AURA Example

Each parachain supplies its own authorizer, and the Parachain Service does not prescribe
one. Its only constraint is that the authorizer's config blob begins with a `Vec<ParaId>`
matching the work package's items (§3.2). What follows is an example AURA-style
collator-set authorizer.

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

- The config blob starts with the `Vec<ParaId>` (`authorized_paras`).
- `len(authorized_paras) == len(workitems)`, rejecting the package otherwise.

#### Anchor Selection and Slot Claiming

The collator picks an anchor block (one of the last 8 JAM blocks) whose timeslot maps
to their collator index. In steady-state AURA the authorizer queue is filled with the
**same** authorizer hash, so the pool's 8 entries are all the same hash and the collator
can pick any of the 8 recent anchors.

For **small collator sets** (< 8 collators), a collator can therefore claim **two
consecutive blocks** by choosing different anchor blocks whose timeslots both map to their
index (e.g. with `collator_set_size = 4` and `slot_duration = 6`, anchor timeslots T and
T+4 both yield the same collator index).

Preventing this is the responsibility of the **parachain's validation code**, not the
authorizer. If the validation code detects that the claimed anchor timeslot is inconsistent
with the parachain's own slot progression (e.g. the same collator claiming back-to-back
slots they are not entitled to), it can call `report_error(data)` to record a structured
complaint against the offending collator in the parachain log, for the parachain's
slashing logic to read.

The mirror case is an author the parachain does not recognise. Anyone can buy coretime on
a core assigned to the parachain and submit whatever they like for it. Here `report_error`
is the wrong tool. There is no known account to slash, so the complaint has no reader, and
writing one would hand the buyer a free way to evict genuine entries from the
capacity-bounded `parachain_log` (§3.1). The validation code should panic instead (§4.2).

#### Filling the 80-slot queue

JAM's `assign` consumes exactly 80 authorizer hashes, one per slot. The Coretime chain
supplies the authorizer set as a queue of length X ≤ 80, and the service fills the 80
slots with the next 80 entries of that set repeated endlessly:

- X = 80, or X < 80 with `80 % X == 0`: the 80 slots tile the set a whole number of
  times, so the installed queue keeps cycling correctly on its own and is written once. A
  handoff to another assigner likewise has to be self-sufficient, so `AssignCore` with a
  `Some` assigner demands an exact 80-hash queue (§4.3).
- X < 80 with `80 % X != 0`: 80 slots do not land on a set boundary, so each cycle must
  resume where the last one stopped. The service keeps the queue and rewrites it every
  80 blocks, shifting its start forward by `80 % X` each time. For X = 11 the first
  cycle is 7 full passes (77) plus authorizers 1 to 3. The next starts at the 4th, runs
  to the 11th, then repeats. The stored order is the schedule, so there is no separate cursor.

#### Collator Set Rotation Flow

```
Parachain runtime
    │  Decides to rotate collator set (e.g. via session change)
    │  Sends XCM to Coretime chain with new collator set root + size
    ▼
Coretime chain
    │  emits AssignCore { core, queue: authorizers, new_assigner: None, jam_slot }
    │  (new authorizer hashes computed from same code + updated config)
    ▼
Parachain Service (Accumulate)
    │  applies at jam_slot. If the set cannot fill 80 exactly it is kept and
    │  re-presented with a rotating partial every 80 blocks (§5.1)
    ▼
Pool (up to 8 entries)
    │  Old authorizer hashes drain out over ~8 blocks (48s)
    │  New ones rotate in
```

### 7.2 On-Demand Parachains

On-demand coretime is not a special case for the Parachain Service. The **Coretime chain**
handles it. When someone buys a single-slot coretime allocation, the Coretime chain emits
`AssignCore { core, queue, new_assigner: None, jam_slot }` with a near-term `jam_slot` to
install the buyer's authorizer on the target core for the duration of that slot. The
Parachain Service only sees a queue update and cannot tell on-demand from bulk-purchased
coretime.

Two possible policies on the Coretime chain side:

- **Direct buyer authorization**: the authorizer for an on-demand slot verifies a
  signature from the buyer's key. The Coretime chain builds the authorizer config with
  the buyer's public key at the time of purchase.
- **Secondary market with pre-registered authorizers**: an off-chain service pre-registers
  generic authorizers on the Coretime chain and resells access tokens off-chain. Whoever
  holds a valid token can then submit work packages against the pre-registered authorizer.

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

UMP (Upward Message Passing) is similarly bounded: `maxUpwardMessageSize` ≈ 64 KiB and
`maxUpwardQueueSize` = 1 MiB on Polkadot mainnet.

On JAM, the buffer between Refine and Accumulate is even tighter. The work-report's
combined successful result blobs plus authorizer trace are bounded by **48 KiB**, and all
upward messages the validation code emits have to fit inside that budget alongside the new
head data. HRMP-style message payloads therefore cannot be carried through the work-report
and need a different channel (§8.2).

### 8.2 Proposed Solution: Full XCMP

The proposed model is **full XCMP**. Refine uses `export()` to write outbound message
payloads into DA segments, so they are distributed off-chain via JAM's data availability
layer (D3L). Accumulate records only message *headers*, *hashes*, and channel metadata
on-chain. This removes the per-message size bottleneck. See
[paritytech/polkadot-sdk#10449](https://github.com/paritytech/polkadot-sdk/pull/10449)
for a potential specification of XCMP.

The host functions for HRMP channel management (open, accept, close) and XCMP message
handling are not yet specified.

---

## 9. References

- [JAM Gray Paper](https://graypaper.com): Formal JAM specification (Gavin Wood)
- [CoreJAM RFC #31](https://github.com/polkadot-fellows/RFCs/pull/31): Original CoreJAM RFC
- [RFC-1: Agile Coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md)
- [RFC-5: Coretime Interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md)
- [Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
- [Polkadot Wiki: JAM Chain](https://wiki.polkadot.network/docs/learn-jam-chain)
- [Demystifying JAM](https://blog.kianenigma.com/posts/tech/demystifying-jam/): Kian Paimani
- [JAM PVM Common API](https://docs.rs/jam-pvm-common/latest/jam_pvm_common/): Host call specifications for Refine and Accumulate
- [JIP-1: Log Host Call](https://github.com/polkadot-fellows/JIPs/blob/main/JIP-1.md): PVM logging specification
