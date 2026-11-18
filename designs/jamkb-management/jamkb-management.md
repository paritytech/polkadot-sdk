# JAMKB Management on Asset Hub

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Asset Hub Components](#3-asset-hub-components)
   - 3.1 [The JAMKB Asset](#31-the-jamkb-asset)
   - 3.2 [`pallet-jamkb`](#32-pallet-jamkb)
     - 3.2.1 [pallet-jamkb precompile](#321-pallet-jamkb-precompile)
   - 3.3 [pallet-assets access](#33-pallet-assets-access)
   - 3.4 [The Generic AH→JAM Transport](#34-the-generic-ahjam-transport)
4. [Allocation Protocols](#4-allocation-protocols)
   - 4.1 [Lease (Supervisor-Managed Allocation)](#41-lease-supervisor-managed-allocation)
   - 4.2 [Permanent Release](#42-permanent-release)
   - 4.3 [Lease Return](#43-lease-return)
   - 4.4 [Voluntary Return](#44-voluntary-return)
5. [Message Protocol](#5-message-protocol)
   - 5.1 [Operations, Correlation](#51-operations-correlation)
   - 5.2 [Memo Requirements](#52-memo-requirements)
6. [References](#6-references)
7. [Open Issues](#7-open-issues)

---

## 1. Overview

This document specifies how JAMKB is managed on Asset Hub.

JAMKB is JAM's resource-access token for state footprint: a JAM service may keep
state up to what its balance covers. Asset Hub maintains a 1:1 representation of
that token and provides the management layer for distribution, leasing, release,
and return.

### Motivation

The DAO distributes JAMKB, but not on JAM directly. JAM has no user accounts
or transactions, and a service balance moves only through the service's own
code. So JAMKB is 1:1 represented on Asset Hub. There, the DAO can distribute it
by referendum or through contracts that automate distribution. The managing
pallet is exposed to contracts through a precompile, which also lets
them lease and redeem. That allows a secondary market to form around the
asset. The scenarios:

- The DOT DAO deploys an adapter contract that sells JAMKB into private
  ownership; the revenue is burned per Referendum 1926.
- A JAM service team buys JAMKB with DOT and redeems it to its service's
  regular balance. After reducing the service's footprint, the team may
  return the units and sell them.
- A parachain team buys JAMKB to back its parachain's state. After
  freeing the state, the team may sell the units.
- A holder keeps and trades JAMKB in ordinary Asset Hub wallets and DEXes,
  or leases it to a JAM service.

### Scope

This document covers:

- The flows: supervisor-managed allocation (lease), permanent release and
  return of funds
- The components they use: the JAMKB asset, `pallet-jamkb` and its precompile

This document does not cover economic policy: how JAMKB is priced, sold or
distributed. Pricing and distribution could live in policy adapters: separate
contracts deployed and replaced by the DAO, holding only a DAO-granted JAMKB
budget. An adapter can implement auctions, broker-style sales, or other
distribution rules. The pallet stays policy-neutral.

---

## 2. Architecture Overview

A JAM service has two balances: a regular balance and a supervisor balance. Both
back the service's state footprint. The service can transfer its regular
balance, but the supervisor balance can be transferred only by the effective
supervisor. In this design the supervisor is the Parachain Service.

At JAM genesis, the Parachain Service is expected to hold all JAMKB units.
Asset Hub manages distribution on behalf of the DAO.

Units sent to JAM are burned on Asset Hub; units that come back are minted.
Once every operation has settled, this holds:

```
PS regular balance = Asset Hub supply + Σ Unclaimed + Σ Excess
```

`Unclaimed` and `Excess` are units booked on Asset Hub and not yet minted
(§3.2).

It can drift in three cases:

- The Parachain Service does not record an incoming transfer: it cannot pay
  for its queue entry, or its processing fails. The funds stay on the
  Parachain Service and Asset Hub does not see them.
- A failure entry is lost before Asset Hub reads it, because it was evicted
  from `parachain_log`. Asset Hub settles a transfer that never happened.
- A holder burns units on Asset Hub with the `pallet-assets` call `refund`.
  The units stay on the Parachain Service.

The first and the third are detected from the Parachain Service balance
(§3.4) and fixed by governance with `force_mint`; the second is prevented by
the log bound (§3.4).

A deferred transfer is discarded if its destination was ejected before it
was accumulated. The units are burnt on JAM. The equation still holds; the
units are gone. For its own transfers, the pallet avoids it on the targets it
ejects (`eject_target`).

Example flow for governance permanent release, from approval through settlement.

```
━━ Asset Hub block B: execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_release(target, amount, memo): records the allocation
   ▼
Any signed account
   │  execute_release(id)
   ▼
pallet-jamkb
   │  burns amount from custody, records the operation and adds a
   │  TransferOut to the send queue of pallet-parachain-system
   ▼
pallet-parachain-system
   │  calls the Parachain Service's send_upward_message(TransferOut{..})

━━ B's work report: Accumulate, on-chain on JAM ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Parachain Service
   │  transfer its balance → target's regular balance
   │  on failure it writes TransferFailed { id, error } to Asset Hub's
   │  parachain_log
   │  records B's header as Asset Hub's para head

━━ Asset Hub block C: a later block, its anchor at or past B's accumulation ━━━━━

pallet-parachain-system
   │  reads the para head and parachain_log; the head is B or later, so B
   │  is accumulated; delivers the operation's outcome with log entries
   ▼
pallet-jamkb
   │  records the outcome against the operation; then settles it:
   │  - Failed → amount is minted back to custody; the allocation returns
   │    to Approved
   │  - Confirmed → the allocation moves to Delivered
```

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. JAMKB has 3 decimals. One plank is one JAM
balance unit, so 1 JAMKB is 1000 units. `CAP` equals 21_000_000 JAMKB
(21 * 10^9 planks). The asset is non-sufficient with `min_balance` 1 plank.

This asset is managed by `pallet-jamkb`, which holds the four privileged
roles (Owner, Issuer, Admin, Freezer), assigned to it at initialization.

`initialize` takes a provider reference on the custody account. Governance
is required to set the metadata (symbol `JAMKB`, decimals 3) with
`force_set_metadata`.

Parachain state footprint is backed by the Parachain Service balance; the
Coretime chain sets each parachain's quota. JAMKB moves to the Coretime chain
by reserve transfer. The units stay on Asset Hub in the Coretime chain's
sovereign account. The Coretime chain never grants more quota than the JAMKB
it holds, and lowers the quota before the units backing it leave, so a
release cannot take the Parachain Service below the parachain quotas.

### 3.2 `pallet-jamkb`

`pallet-jamkb` manages JAMKB on Asset Hub and executes asynchronous
cross-system operations through `pallet-parachain-system`.

It keeps the DAO's undistributed units in one keyless account derived from
its `PalletId`, `py/jamkb`: `custody`, `into_account_truncating()`.

A call that records a transfer to JAM burns the units it sends. Units that
come back to the Parachain Service are minted (`Reclaim`, `Eject`, §4.4).
A mint can fail, for example into an account with no JAMKB account and no
native balance to create one. The pallet books the amount of a failed mint
as `Unclaimed` for that account (`UnclaimedBooked`), minted later by
`claim`. Every cross-system operation is asynchronous and is finalized by
settlement in `on_poll`. On failure the pallet undoes what the call did,
unless the payload says otherwise.

To resolve operations, the pallet receives each operation's outcome from the
parachain-system pallet (§3.4): the timeslot it was accumulated at and its
`parachain_log` entries. A failure entry marks it `Failed`, otherwise it is
`Confirmed` (§5.1). The operations are settled in `on_poll` within its
weight budget. A failed settlement is retried by `settle` only. At most
`MAX_UNSETTLED_OPERATIONS` operations are unsettled; past the limit a call
that records one is rejected with `TooManyOperations`.

A `Delivery`, `Increase` or `Reclaim` is an allocation operation; a `Freeze`,
`Unfreeze`, `Cleanup`, `Forget`, `Eject` or `Unsupervise` is a recovery
operation. The pallet keeps at most one unsettled operation per allocation
and at most one unsettled recovery operation per target; a call that would
record a second is rejected with `UnsettledOperation`.

For voluntary return (§4.4) it receives each `incoming_transfers` bucket
from parachain-system and books it.

A transfer may land on the Parachain Service's supervisor balance. For each
bucket with such transfers, the pallet records one `Move` of their sum from
the supervisor balance to the regular balance, and books them when the `Move`
is confirmed.

The pallet starts inactive. Governance reads the Parachain Service's regular
balance from public JAM state at a defined block whose `incoming_transfers`
queue is empty. It records it with `attest(anchor, released)`, where
`released` is `CAP` minus that balance, 0 when everything is on the Parachain
Service; the pallet stores the anchor and mints `CAP − released` into
custody. Until attested, every call except `initialize` and `attest` is
rejected.

```rust
/// The header hash of the JAM block whose state the balances were checked
/// against. The pallet records it and does not verify it.
type Anchor = Hash;

/// The JAMKB units on JAM at genesis, in planks (§3.1).
const CAP: Balance;

/// The minimum `amount` that can be transferred by
/// `approve_release`, `redeem`, `offer_lease`, `increase_lease`, `reclaim`,
/// and `dispose_excess` with `refund`. A smaller `amount` is rejected,
/// except a `Full` reclaim.
const MIN_TRANSFER: Balance;

/// The gas every deferred `TransferOut` carries (§5.2). A destination whose
/// `min_memo_gas` is above it refuses with `GasBelowDestinationMinimum`, and
/// the operation fails.
const MEMO_GAS: u64;

/// The weight `on_poll` spends per block settling operations (§3.2). At
/// least the settlement of the operations one block's send share can carry
/// (§3.4), an `Eject` with `MAX_LEASES_PER_TARGET` leases included.
const SETTLEMENT_WEIGHT: Weight;

/// The most operations unsettled at once (§3.2): a bound on storage. Above
/// what the transport can have queued and in flight (§3.4) and what can be
/// resolved and not yet settled (`SETTLEMENT_WEIGHT`), so it is reached
/// only through failed settlements. During a multi-block migration `on_poll`
/// does not run. Everything in flight can be resolved and wait; the bound
/// covers it.
const MAX_UNSETTLED_OPERATIONS: u32;

/// A code hash with no preimage; a target set to it cannot run (§4.3).
///
/// Used to prevent an uncooperative lease holder from executing again.
const FROZEN_CODE_HASH: Hash = [0u8; 32];

type AllocationId = u64;              // assigned from 1; never reused
type OperationId = u64;

/// A JAM service index.
type ServiceId = u32;

/// The runtime's `pallet-assets` asset id.
type AssetId;

/// A JAM timeslot. All times and durations of the pallet are in timeslots.
type Timeslot = u32;

/// Added to the lease end (`reclaim`, §4.3).
const LEASE_GRACE_PERIOD: Timeslot;

/// Cooldown after a Failed reclaim. Blocks reclaims by anyone except the
/// approver or governance (`reclaim`, §4.3).
const RECLAIM_COOLDOWN: Timeslot;

/// The cap on a target's accepted leases: those in `Delivering`, `Delivered`
/// or `Reclaiming`.
const MAX_LEASES_PER_TARGET: u32;

/// The recovery deposit (§4.3): a hold on the caller's native balance per
/// `forget_preimage` call.
const RECOVERY_DEPOSIT: Balance;
/// The recovery deposit per key of a `cleanup_storage` page (§4.3).
const RECOVERY_DEPOSIT_PER_KEY: Balance;

/// The longest raw storage key of a supervised service (`cleanup_storage`,
/// §4.3).
const MAX_KEY_LEN: u32;
/// The most keys one `cleanup_storage` call removes. Each key is one message
/// and can cause one failure entry, so a full page fits the
/// transport's budget with nothing outstanding (§3.4).
const MAX_KEYS_PER_PAGE: u32;
type Key = BoundedVec<u8, MAX_KEY_LEN>;

/// The Asset Hub account associated with a JAM service.
type ServiceAccounts = Map<ServiceId, AccountId>;

/// Units whose mint to the beneficiary failed (§3.2), awaiting `claim`.
type Unclaimed = Map<AccountId, Balance>;

/// Unattributed inflows, per source service (§4.4).
type Excess = Map<ServiceId, Balance>;

/// The frozen lease targets (§4.3).
type FrozenTargets = Map<ServiceId, ()>;

/// The `due` of a Confirmed `Forget`, by target and (hash, len): the
/// timeslot after which a second `forget_preimage` expunges the preimage
/// (§4.3).
type ForgetDue = DoubleMap<ServiceId, (Hash, u32), Timeslot>;

/// One approved allocation.
struct Allocation {
    id: AllocationId,
    mode: AllocationMode,             // Lease { duration, reclaim_from }
                                      // | Permanent { memo }
    target: ServiceId,
    /// The units of the allocation. Held by `offer_lease`; burned when the
    /// delivery is recorded (`execute_release`, `accept_lease`).
    amount: Balance,
    state: AllocationState,
    delivered_at: Option<Timeslot>,
                                      // the timeslot the delivery was
                                      // accumulated at. A lease ends at
                                      // `delivered_at + duration`
    approver: Option<AccountId>,      // the signing account or contract;
                                      // `None`: governance, for a permanent
                                      // release
    valid_from: Timeslot,             // `accept_lease` and `execute_release`
                                      // are rejected before this timeslot
    expires_at: Timeslot,             // `valid_from + valid_for`;
                                      // `accept_lease` and `execute_release`
                                      // are rejected from this timeslot on
}

enum AllocationMode {
    Lease {
        duration: Timeslot,
        reclaim_from: Option<Timeslot>,   // end of the cooldown
                                          // (`reclaim`)
    },
    Permanent { memo: [u8; 128] },    // the memo of the release (§5.2)
}

/// What a `reclaim` takes back.
enum ReclaimAmount {
    Full,                             // the allocation's whole `amount`
    Partial(Balance),                 // at least `MIN_TRANSFER`
}

/// A cancelled, returned, ejected, closed or undelivered allocation is
/// removed.
enum AllocationState {
    Approved,
    Delivering,                       // credit in flight (lease or release)
    Delivered,                        // the delivery settled Confirmed. For a
                                      // release it is terminal: the DAO no
                                      // longer controls the units. For a
                                      // lease, reclaim may follow
    Reclaiming,                       // entered by a Failed `Reclaim`;
                                      // Lease only
}

/// One cross-system operation between Asset Hub and JAM.
struct Operation {
    id: OperationId,                  // unique operation Id (§5.1)
    payload: OperationPayload,
    state: OperationState,
    accumulated_at: Option<Timeslot>,
                                      // the timeslot it was accumulated at
                                      // (§3.4); unset until reported
    settlement_failed: bool,          // set by a failed settlement
                                      // (`SettlementFailed`); `on_poll`
                                      // skips it, `settle` retries it
}

/// The operation's data payload.
enum OperationPayload {
    /// A lease or a governance permanent release (§4.1, §4.2.1).
    /// One `TransferOut` from the Parachain Service's regular balance. A lease
    /// credits the target's supervisor balance. A permanent release credits
    /// the target's regular balance and is deferred, with the memo and the
    /// gas of §5.2.
    ///
    /// Confirmed: the allocation moves to `Delivered` and `delivered_at` is
    /// set to the operation's `accumulated_at`.
    /// Failed: a release's units are minted back to custody and it returns
    /// to `Approved`; a lease is removed and its units are minted to the
    /// approver (§3.2).
    Delivery {
        allocation: AllocationId,
        amount: Balance,              // the units burned for the transfer
    },
    /// A lease increase (`increase_lease`).
    /// One plain `TransferOut` from the Parachain Service's regular balance to
    /// the target's supervisor balance.
    ///
    /// Confirmed: the allocation's amount rises by `additional`; the lease
    /// stays `Delivered`.
    /// Failed: `additional` is minted back to the approver (§3.2).
    Increase {
        allocation: AllocationId,
        additional: Balance,
    },
    /// A redemption of holder funds (§4.2.2).
    /// One deferred `TransferOut` from the Parachain Service's regular balance
    /// to `target`'s regular balance, with the memo and gas of §5.2.
    ///
    /// Confirmed: no further state change.
    Redemption {
        holder: AccountId,            // the redeemer
        target: ServiceId,
        amount: Balance,              // the units burned for the transfer
        memo: [u8; 128],              // the redeemer's memo (§5.2)
    },
    /// A lease return (`reclaim`, §4.3).
    /// One plain `TransferOut` from the lease target's supervisor balance to
    /// the Parachain Service's regular balance.
    ///
    /// Confirmed: `amount` is minted to the approver and the allocation's
    /// amount falls by `amount`; a fully returned lease is removed.
    /// Failed: a `Reclaim` whose `accumulated_at` is past the lease end and
    /// `LEASE_GRACE_PERIOD` moves a `Delivered` lease to `Reclaiming`;
    /// otherwise the lease keeps its state. A Failed `Reclaim` not
    /// `privileged` sets `reclaim_from` to `accumulated_at +
    /// RECLAIM_COOLDOWN`.
    Reclaim {
        allocation: AllocationId,
        amount: Balance,
        privileged: bool,             // by the approver or governance
    },
    /// A freeze (`freeze_target`, §4.3).
    /// One `UpgradeService { service: Some(target), code_hash:
    /// FROZEN_CODE_HASH, len: 0, min_acc_gas: 0, min_memo_gas: 0 }`.
    ///
    /// Confirmed: the target is added to `FrozenTargets`.
    /// Failed: does not occur. The Parachain Service logs no failure for an
    /// `UpgradeService`.
    Freeze {
        target: ServiceId,
    },
    /// An unfreeze (`unfreeze_target`, §4.3).
    /// One `UpgradeService { service: Some(target), code_hash, len: 0,
    /// min_acc_gas, min_memo_gas }` with the values the caller passed.
    ///
    /// Confirmed: the target is removed from `FrozenTargets`.
    /// Failed: does not occur, as for `Freeze`.
    Unfreeze {
        target: ServiceId,
        code_hash: Hash,
        min_acc_gas: u64,
        min_memo_gas: u64,
    },
    /// A cleanup page (`cleanup_storage`, §4.3).
    /// One `RemoveServiceStorage { service: target, key }` per key in `keys`.
    ///
    /// Confirmed: the deposit is released.
    /// Failed: the keys that were present are removed regardless;
    /// `RECOVERY_DEPOSIT_PER_KEY` is slashed to the treasury per missing key
    /// and the rest of the deposit is released.
    Cleanup {
        target: ServiceId,
        keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>,
        deposit: (AccountId, Balance),
    },
    /// A preimage release (`forget_preimage`, §4.3).
    /// One `Forget { target: Service(target), hash, len }`.
    ///
    /// Confirmed: the deposit is released. The first call stores the `due` of
    /// its `ForgetAgainAt` in `ForgetDue`. The second call expunges the
    /// preimage and removes the `ForgetDue` entry.
    /// Failed: the deposit is slashed to the treasury.
    Forget {
        target: ServiceId,
        hash: Hash,
        len: u32,
        deposit: (AccountId, Balance),
    },
    /// An ejection (`eject_target`, §4.3).
    /// One `EjectService { service: target }`. It fails `NotEmpty` until the
    /// target holds no storage items and no preimage requests.
    ///
    /// Confirmed, with `ServiceEjected { credited }`: every accepted lease
    /// on the target is removed and its `amount`, or what is left of
    /// `credited`, is minted to its approver; the rest of
    /// `credited` is recorded as `Excess[target]` (`ExcessRecorded`). The
    /// target's `FrozenTargets` and `ServiceAccounts` entries are removed.
    /// Confirmed without a `ServiceEjected` entry: the settlement fails
    /// until `force_settle` supplies it.
    /// Failed: every lease keeps its state.
    Eject {
        target: ServiceId,
    },
    /// A release from supervision (`unsupervise`, §4.3).
    /// One `SetServiceSupervisor { service: target, new_supervisor: target }`.
    ///
    /// Confirmed and Failed: no further state change.
    Unsupervise {
        target: ServiceId,
    },
    /// A move of a bucket's transfers from the Parachain Service's supervisor
    /// balance to its regular balance (§3.2). One plain `TransferOut` of their
    /// sum between the two. It cannot fail while the Parachain Service is its
    /// own supervisor and nothing else spends its supervisor balance (§7.5).
    ///
    /// Confirmed: each transfer is booked (§4.4).
    /// Failed: nothing is booked; the units stay on the supervisor balance.
    Move {
        transfers: IncomingTransfers,
    },
    /// A refund of an incoming transfer to its source (`dispose_excess`).
    /// One deferred `TransferOut` from the Parachain Service's regular balance
    /// to `source`'s regular balance, with the memo and gas of §5.2.
    ///
    /// Confirmed: no further state change.
    /// Failed: `Excess[source]` is restored.
    Refund {
        source: ServiceId,
        amount: Balance,
        memo: [u8; 128],              // governance's memo (§5.2)
    },
}

/// The state of an unsettled operation; a settled one is removed.
enum OperationState {
    Requested,                        // recorded and queued
    Confirmed(Option<SuccessEntry>),  // resolved to success; not settled yet
    Failed(FailureReason),            // resolved to failure; not settled yet
}

/// The success entry the settlement reads, named as in the Parachain Service
/// design.
enum SuccessEntry {
    ServiceEjected { credited: Balance }, // ServiceEjected { service, credited }; Eject
    ForgetAgainAt { due: Timeslot },      // ForgetAgainAt { target, hash, len, due }; Forget
}

/// The failure carried by `OperationFailed`: one variant per failure entry
/// class, carrying that entry's detail (Parachain Service design).
enum FailureReason {
    TransferFailed(TransferError),                   // TransferFailed { id, error }
    /// One `ServiceKeyNotFound { service, key_hash }` per missing key.
    ServiceKeyNotFound(BoundedVec<Hash, MAX_KEYS_PER_PAGE>),
    /// `ServiceForgetNotRequested { service, hash, len }`.
    ServiceForgetNotRequested,
    ServiceEjectFailed(ServiceEjectError),           // ServiceEjectFailed { service, error }
    ServiceSupervisorFailed(ServiceSupervisorError), // ServiceSupervisorFailed { service, error }
}

/// The booking class of a return (§4.4).
enum ReturnClass {
    Credited(AccountId),  // minted to the beneficiary
    Unclaimed(AccountId), // the beneficiary cannot receive a mint; `claim`
    Excess,               // owner unknown, disposed by governance
}

/// Errors.
enum Error {
    NotAttested, Paused, BelowMinTransfer, WrongMode, WrongState, NotYetValid,
    Expired, NoServiceAccount, TargetIsParachainService, TooManyLeases,
    UnsettledOperation, NotFrozen, AlreadyFrozen, EjectUnsettled,
    SettlementNotFailed, TooManyOperations, Transport,
    UnknownAllocation, UnknownOperation, DurationNotIncreased,
    InsufficientExcess, InsufficientBalance, NothingToClaim, HasLeases,
    DuplicateKey, ReclaimCooldown, FullReclaimOnly, ReleasedAboveCap,
    AlreadyInitialized, NotInitialized, AlreadyAttested, ForgetNotDue,
}
```

Typed events:

```rust
enum Event {
    /// The JAMKB asset is created with the id `pallet-assets` allocated.
    Initialized { asset: AssetId },
    /// The genesis attestation is recorded.
    Attested { anchor: Anchor, released: Balance },
    ReleaseApproved { id: AllocationId, target: ServiceId, amount: Balance },
    LeaseOffered { id: AllocationId, target: ServiceId, amount: Balance,
                   duration: Timeslot },
    AllocationCancelled { id: AllocationId },
    /// A lease was removed (a Failed `Delivery`, `Reclaim`, `Eject`,
    /// `close_lease`); `amount` is what was not returned.
    LeaseClosed { id: AllocationId, amount: Balance },
    LeaseIncreased { id: AllocationId, additional: Balance },
    LeaseExtended { id: AllocationId, duration: Timeslot },
    /// `allocation` is `None` for a redemption, a refund, a move or a recovery
    /// operation.
    OperationRequested { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId, entry: Option<SuccessEntry> },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// The settlement of `id` failed with `error`; `settle` retries it
    /// (§3.2).
    SettlementFailed { id: OperationId, error: DispatchError },
    /// An amount was booked (§4.4) as `class`, with the raw `memo` of the
    /// return. A return booked as `Unclaimed` also emits `UnclaimedBooked`
    /// (§3.2). A transfer to the supervisor balance is booked when its
    /// `Move` is confirmed.
    ReturnBooked { source: ServiceId, amount: Balance, class: ReturnClass,
                   memo: [u8; 128] },
    /// A mint to `beneficiary` failed (§3.2); the amount awaits `claim`.
    UnclaimedBooked { beneficiary: AccountId, amount: Balance },
    /// A beneficiary's unclaimed balance was claimed.
    Claimed { beneficiary: AccountId, amount: Balance },
    TargetFrozen { target: ServiceId },
    TargetUnfrozen { target: ServiceId },
    BudgetGranted { beneficiary: AccountId, amount: Balance },
    ExcessDisposed { source: ServiceId, amount: Balance },
    /// An `Eject` recorded `amount` as `Excess[source]`.
    ExcessRecorded { source: ServiceId, amount: Balance },
    ForceMinted { anchor: Anchor, beneficiary: AccountId, amount: Balance },
    Paused,
    Resumed,
}
```

Every call returns `DispatchResult`. New allocation and operation ids are
reported in the events. A signed call that records an operation pays the
delivery fee of its messages (§3.4). A governance call and a `Move` pay no
fee.

The common errors are listed in `Error`; a call's doc names only the errors
specific to it.

```rust
// ── Setup and control ──────────────────────────────────────────────────────

/// Creates the JAMKB asset (`Initialized`). Origin: governance (Root). Maps
/// the custody account in pallet-revive without a deposit. Rejected with
/// `AlreadyInitialized` when called again.
fn initialize();

/// Records the genesis attestation against JAM state. Origin: governance
/// (Root). `released` is `CAP` minus the Parachain Service's regular balance
/// at `anchor` (§3.2); `CAP − released` is minted into custody. Enables
/// operations. Rejected with `ReleasedAboveCap` when `released` is above
/// `CAP`, with `NotInitialized` before `initialize` and with
/// `AlreadyAttested` when called again.
fn attest(anchor: Anchor, released: Balance);

/// Sets the pallet pause flag. Origin: governance (Root). While set, a call
/// that creates an allocation, records an operation or distributes units from
/// custody is rejected. Messages already queued are still processed.
fn pause();

/// Clears the pallet pause flag. Origin: governance (Root).
fn resume();

/// Corrects the Asset Hub supply (§2): an unrecorded return or a holder's
/// burn. Origin: governance (Root). Mints `amount` to
/// `beneficiary`. `anchor` is the JAM block whose balances show the
/// difference; it is reported in `ForceMinted`.
fn force_mint(anchor: Anchor, beneficiary: AccountId, amount: Balance);

// ── Distribution from custody ──────────────────────────────────────────────

/// Grants a budget to a policy adapter contract (§1). Origin: governance
/// (Root). Transfers `amount` of undistributed units from the pallet's
/// custody to `beneficiary` on Asset Hub. A budget can return by an
/// ordinary transfer to `custody`.
fn grant(beneficiary: AccountId, amount: Balance);

// ── Permanent release ──────────────────────────────────────────────────────

/// Approves a permanent release to `target`. Origin: governance (Root).
/// Records `Allocation{mode: Permanent { memo }, state: Approved}`. `memo`
/// goes to `target` unchanged (§5.2).
/// `valid_from` `None` means the current timeslot. Rejected when `target` is the Parachain Service.
fn approve_release(target: ServiceId, amount: Balance, memo: [u8; 128],
                   valid_from: Option<Timeslot>, valid_for: Timeslot);

/// Executes an approved release. Origin: any signed account. Legal while
/// the allocation is `Approved`. Burns `amount` from custody, moves the
/// allocation to `Delivering` and records a `Delivery` operation. Rejected
/// with `InsufficientBalance` when custody cannot cover `amount`.
fn execute_release(id: AllocationId);

/// Moves the caller's own units to `target`. Origin: any signed account. Burns
/// `amount` from the caller and records a `Redemption` operation with the
/// caller as `holder`. `memo` goes to `target` unchanged (§5.2). Rejected
/// when `target` is the Parachain Service,
/// and with `InsufficientBalance` when the caller cannot cover `amount`.
fn redeem(amount: Balance, target: ServiceId, memo: [u8; 128]);

// ── Lease ──────────────────────────────────────────────────────────────────

/// Offers a lease of `amount` to `target` for `duration`. Origin: a token
/// holder. Holds `amount` on the caller (`LeaseOffer`) and records
/// `Allocation{mode: Lease, state: Approved}`. `valid_from` `None` means the
/// current timeslot. Rejected with `InsufficientBalance` when the caller
/// cannot cover `amount`, with `NoServiceAccount` when the target has no
/// service account and when it is the
/// Parachain Service.
fn offer_lease(target: ServiceId, amount: Balance, duration: Timeslot,
               valid_from: Option<Timeslot>, valid_for: Timeslot);

/// Accepts an offered lease. Origin: the target's service account
/// (`ServiceAccounts`). Legal while the allocation is `Approved`. Rejected
/// with `TooManyLeases` while the target has `MAX_LEASES_PER_TARGET`
/// accepted leases. Burns the held `amount`, moves the allocation to
/// `Delivering` and records a `Delivery` operation.
fn accept_lease(id: AllocationId);

/// Raises a lease's `amount` by `additional` funds. Origin: the lease's
/// approver. Legal while the lease is `Delivered` and before the lease end.
/// Burns `additional` from the approver and records an `Increase`
/// operation. The lease end does not change. Rejected with
/// `InsufficientBalance` when the approver cannot cover `additional`.
fn increase_lease(id: AllocationId, additional: Balance);

/// Sets the lease's `duration`. Origin: the lease's approver. Legal while the
/// lease is `Delivered` and before the lease end. Rejected with
/// `DurationNotIncreased` unless `duration` is greater than the current one.
fn extend_lease(id: AllocationId, duration: Timeslot);

/// Takes `amount` of the leased units back from the target. The approver,
/// the target's service account and governance can reclaim `Full` or
/// `Partial` at any time. From the lease end plus `LEASE_GRACE_PERIOD` on
/// anyone can reclaim `Full`; a partial reclaim by them is rejected with
/// `FullReclaimOnly`. Legal while the lease is `Delivered` or `Reclaiming`.
/// Before `reclaim_from` only the approver and governance can reclaim; other
/// calls are rejected with `ReclaimCooldown`. Records a `Reclaim` operation.
fn reclaim(id: AllocationId, amount: ReclaimAmount);

/// Ends a lease that was not fully returned. Origin: the lease's approver or
/// governance. Legal while the lease is `Reclaiming`. Rejected with
/// `UnsettledOperation` while the lease has an unsettled allocation
/// operation (§3.2). The lease is removed; the units not returned stay
/// on the target's supervisor balance.
fn close_lease(id: AllocationId);

// ── Recovery: a lease target that does not cooperate ───────────────────────

/// Stops the target from taking more state footprint: its code hash is set
/// to `FROZEN_CODE_HASH`, so it no longer runs. Its code preimage stays in
/// place for `unfreeze_target`. Origin: the approver of a lease
/// on the target, or governance. Legal while a lease on the target is
/// `Reclaiming`. Rejected with `AlreadyFrozen` while the target is recorded
/// frozen. Records a `Freeze` operation.
fn freeze_target(target: ServiceId);

/// Restores the target: sets its code hash and gas limits to the values
/// passed. Origin: the target's service account or governance. Legal while
/// no lease on the target is `Reclaiming`. Rejected with `NotFrozen` unless
/// the target is recorded frozen. Records an `Unfreeze` operation.
fn unfreeze_target(target: ServiceId, code_hash: Hash, min_acc_gas: u64,
                   min_memo_gas: u64);

/// Deletes one bounded page of keys from the target's storage. Origin: any
/// signed account, with `RECOVERY_DEPOSIT_PER_KEY` per key. Legal while a
/// lease on the target is `Reclaiming`. Rejected with `NotFrozen` unless the
/// target is recorded frozen, and with `DuplicateKey` if a key repeats.
/// Records a `Cleanup` operation.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Releases a solicited preimage of the target. Origin: any signed account,
/// with `RECOVERY_DEPOSIT`. Legal while a lease on the target is `Reclaiming`.
/// Rejected with `NotFrozen` unless the target is recorded frozen. A provided
/// preimage is expunged by a second call accumulated after the `due` of the
/// first's `ForgetAgainAt` (Parachain Service design §6.1); until `ForgetDue`
/// has passed the second call is rejected with `ForgetNotDue`. Records a
/// `Forget` operation.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Destroys the emptied target, crediting its regular and supervisor
/// balances to the Parachain Service's regular balance. Origin: the approver
/// of a lease on the target, or governance. Legal while a lease
/// on the target is `Reclaiming`. Rejected with `UnsettledOperation`
/// while a lease on the target has an unsettled allocation operation, or any
/// other transfer to the target is unsettled (§3.2). Records an `Eject`
/// operation.
/// While the `Eject` is unsettled, `accept_lease`, `increase_lease`,
/// `extend_lease`, `reclaim` and `close_lease` on the target are rejected
/// with `EjectUnsettled`.
fn eject_target(target: ServiceId);

/// Releases the supervised target to itself. Origin: the target's service
/// account or governance. Rejected with `AlreadyFrozen` while the target is
/// recorded frozen and with `HasLeases` while it has an accepted lease.
/// Records an `Unsupervise` operation.
fn unsupervise(target: ServiceId);

// ── Returns and disposal ───────────────────────────────────────────────────

/// Claims the voluntarily returned balance to `beneficiary`'s account (§4.4).
/// Origin: any signed account. Mints `Unclaimed[beneficiary]` to
/// `beneficiary`. Rejected with `NothingToClaim` when it is zero, and with
/// the mint's error when the mint fails (§3.2); the unclaimed balance stays
/// booked.
fn claim(beneficiary: AccountId);

/// Disposes an excess amount per service. Origin: governance (Root). Lowers
/// `Excess[source]` by `amount`. With `refund = true` it records a `Refund`
/// operation sending the units back to the `source` service's regular
/// balance, with `memo` unchanged (§5.2); with `refund = false` it mints the
/// units to custody and `memo` is ignored. Rejected with `InsufficientExcess`
/// when `Excess[source]` is below `amount`.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool,
                  memo: [u8; 128]);

// ── Shared by every flow ───────────────────────────────────────────────────

/// Cancels an allocation in `Approved`. Origin: the allocation's approver,
/// governance for a permanent release, or any signed account from
/// `expires_at` on. The allocation is removed; a lease's hold is
/// released. A `Delivering` allocation cannot be cancelled.
fn cancel_allocation(id: AllocationId);

/// Retries a failed settlement (§3.2). Origin: any signed account.
/// Rejected with `SettlementNotFailed` unless the operation's settlement has
/// failed.
fn settle(op_id: OperationId);

/// Settles an operation with the entry governance supplies, for an outcome
/// whose entry was evicted (`Eject`). Origin: governance (Root). Rejected
/// with `SettlementNotFailed` unless the operation's settlement has failed.
fn force_settle(op_id: OperationId, entry: Option<SuccessEntry>);
```

#### 3.2.1 pallet-jamkb precompile

Contracts access the pallet through a precompile at a fixed address. It
exposes the calls and reads a contract needs. Each precompile function calls
the internal pallet method that holds the logic shared with the dispatchable
of the same name. The call origin is the account that calls the precompile.
Every account in the interface is an AccountId32 passed as `bytes32`. For a
contract, and any unmapped Ethereum account, it is `H160 ++ 0xEE * 12`,
which the System precompile's `toAccountId` returns. A left-padded
`bytes32(uint256(uint160(a)))` is a different account that nobody controls;
a voluntary return with it as the beneficiary loses the units.

No governance call is exposed, and a Root caller is rejected. The precompile
rejects a delegate call and a state-changing call in a read-only frame. It
returns the new allocation id or the new operation id. On failure the call
reverts with the pallet error name. The precompile does not emit Solidity
events. A contract follows its operation with `operationState`. A settled
operation is removed. Its outcome is read from the state it changed: the
allocation's state and amount, `isFrozen`, `leasesOf`, `serviceAccount` or
the caller's balance. A `Cleanup`, `Forget` or `Unsupervise` changes no such
state.

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

address constant JAMKB_ADDR = 0x0000000000000000000000000000000009030000;

interface IJamkb {
    enum AllocationState { NotFound, Approved, Delivering, Delivered,
                           Reclaiming }
    enum AllocationMode { Lease, Permanent }
    enum OperationState { NotFound, Requested, Confirmed, Failed,
                          SettlementFailed }

    struct Allocation {
        uint64 id;
        AllocationState state;
        AllocationMode mode;
        uint32 target;
        uint128 amount;
        bytes32 approver;         // bytes32(0) for governance
        uint32 deliveredAt;       // 0: not delivered
        uint32 duration;          // 0 for Permanent
        uint32 reclaimFrom;       // 0: none
        uint32 validFrom;
        uint32 expiresAt;
    }

    // -- Permanent release --
    function executeRelease(uint64 allocationId) external returns (uint64 opId);
    function redeem(uint128 amount, uint32 target, bytes32[4] calldata memo)
        external returns (uint64 opId);

    // -- Lease --
    /// `validFrom` 0 means the current timeslot.
    function offerLease(uint32 target, uint128 amount, uint32 duration,
                        uint32 validFrom, uint32 validFor)
        external returns (uint64 allocationId);
    function acceptLease(uint64 allocationId) external returns (uint64 opId);
    function increaseLease(uint64 allocationId, uint128 additional)
        external returns (uint64 opId);
    function extendLease(uint64 allocationId, uint32 duration) external;
    function reclaim(uint64 allocationId, uint128 amount) external returns (uint64 opId);
    function reclaimAll(uint64 allocationId) external returns (uint64 opId);
    function closeLease(uint64 allocationId) external;

    // -- Recovery --
    function freezeTarget(uint32 target) external returns (uint64 opId);
    function unfreezeTarget(uint32 target, bytes32 codeHash,
        uint64 minAccGas, uint64 minMemoGas) external returns (uint64 opId);
    function cleanupStorage(uint32 target, bytes[] calldata keys)
        external returns (uint64 opId);
    function forgetPreimage(uint32 target, bytes32 hash, uint32 len)
        external returns (uint64 opId);
    function ejectTarget(uint32 target) external returns (uint64 opId);
    function unsupervise(uint32 target) external returns (uint64 opId);

    // -- Returns --
    function claim(bytes32 beneficiary) external returns (uint128 claimed);

    // -- Shared --
    function cancelAllocation(uint64 allocationId) external;
    function settle(uint64 opId) external;

    // -- Reads. --
    /// `NotFound`, with every other field zero, for an id the pallet does
    /// not hold: removed, or never issued.
    function allocation(uint64 allocationId) external view returns (Allocation memory);
    /// `NotFound` for an id the pallet does not hold: settled, or never issued.
    /// `SettlementFailed` while `settle` is needed.
    function operationState(uint64 opId) external view returns (OperationState);
    function claimableOf(bytes32 beneficiary) external view returns (uint128);
    /// `exists` is false when the target has none.
    function serviceAccount(uint32 target)
        external view returns (bool exists, bytes32 account);
    function isFrozen(uint32 target) external view returns (bool);
    /// The target's accepted leases: `Delivering`, `Delivered` or `Reclaiming`.
    /// At most `MAX_LEASES_PER_TARGET`. Offers are not listed.
    function leasesOf(uint32 target) external view returns (uint64[] memory allocationIds);
    function gracePeriod() external view returns (uint32);
    function reclaimCooldown() external view returns (uint32);
    function minTransfer() external view returns (uint128);
    function maxLeasesPerTarget() external view returns (uint32);
    function recoveryDeposit() external view returns (uint128);
    function recoveryDepositPerKey() external view returns (uint128);
    function maxKeysPerPage() external view returns (uint32);
    function currentTimeslot() external view returns (uint32);
}
```

### 3.3 pallet-assets access

`pallet-assets` holds JAMKB. `pallet-jamkb` administers it through the
runtime-internal fungibles traits: `transfer`, `burn_from`, `mint_into`,
`hold`, `release` and `burn_held`. Every transfer or burn from custody uses
`Preserve`, so custody keeps `min_balance` and its asset account is never
removed. Policy adapters move their JAMKB balances through the ERC20
precompile.

The recovery deposit is a hold on the native currency through
`pallet-balances`; the lease offer is a hold on JAMKB through
`pallet-assets-holder`:

```rust
/// The hold classes.
enum HoldReason {
    /// The native-currency recovery deposit (`RECOVERY_DEPOSIT`).
    #[codec(index = 0)]
    RecoveryDeposit,
    /// The offered units of a lease (`offer_lease`), on JAMKB.
    #[codec(index = 1)]
    LeaseOffer,
}
```

A hold cannot take an account's last plank: it keeps `min_balance` free.

### 3.4 The Generic AH→JAM Transport

The pallet contains no code that reaches the Parachain Service directly. It
is layered on a generic transport mechanism, expected as an extension of
`parachain-system` into a generic AH→JAM transport pallet. The
[cumulus-on-jam](https://github.com/paritytech/polkadot-sdk/pull/12714) §11
`validate_block` rework covers only the outputs a block emits through host
calls; the service-state read is undefined there. The generic pallet
would send the upward messages through `send_upward_message` inside
`jam_validate_block` (Parachain Service design §4.3), read the Parachain
Service state at the block's anchor, and call the pallet's hooks.

#### The parachain-system pallet requirements

The design of that pallet is outside the scope of this document. The
requirements below are what the pallet needs from it to operate.

The pallet-jamkb queues messages in the parachain-system pallet. The
parachain-system pallet reports each operation's outcome and delivers the
`incoming_transfers` buckets through hooks to pallet-jamkb where they are
processed.

It holds the send queue:
`pallet-jamkb` adds each operation's messages to it, and it sends them
through `send_upward_message` within the limits below.

The transport layer responsibilities:

- the current timeslot: the timeslot of the block's anchor, not lower than
  the parent's;
- the outcome of each operation: once, in send order, in the first block
  whose anchor has the sending block accumulated, with its `AccumulateLog`
  events from the log entries past the parent's anchor;
  `accumulated_at`, the timeslot it was accumulated at (§7.6);
- `incoming_transfers` delivered as whole buckets, up to the number
  `pallet-jamkb` can process; a bucket not processed is delivered again;
- `CleanUpBucketsUpTo` sent after the processed `incoming_transfers` buckets;
- a send queue within 1024 messages and 40 KiB per block, FIFO, with a
  maximum queued size and a sharing policy between pallets; a pallet's
  share fits any single operation, a full `Cleanup` page included;
- a delivery fee factor that grows geometrically with the queued size, as
  `UpwardDeliveryFeeFactor` does for upward messages; every sending pallet
  charges the fee to its signed caller;
- gas accounting for the send queue messages: the base cost and the
  messages of one block fit the accumulate gas its work item declares,
  shared between pallets; a block over it is skipped whole. Each message
  costs its replay; a `TransferOut` adds `C_gasT` (Gray Paper), plus the
  memo gas if deferred. The limit is large enough for any single operation,
  so every queued message is sent; messages that do not fit in this block
  are sent in a later one;
- a bound on the log entries not yet pruned: each message counts the
  largest `AccumulateLog` event it can cause, and each block its
  `LogEntry::Accumulate`, until an Asset Hub block whose anchor is past
  them is accepted; the messages of one operation are sent in one
  block; the total stays within the 64 KiB log, so no entry is evicted
  before Asset Hub reads it;

---

## 4. Allocation Protocols

This section shows the end-to-end protocol only. Origins, validity rules and
immediate local effects are defined in the call reference in §3.2.
Unless stated otherwise, once a call records an operation, the generic
transport in §3.4 later sends the queued message and the operation settles
(§3.2).

### 4.1 Lease (Supervisor-Managed Allocation)

A lease is a token transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor, and
the target has a service account (§3.2).

```
Phase 1: Offer        `offer_lease(target, amount, duration, ..)` holds the
                      units and records the lease offer.
Phase 2: Accept       `accept_lease(id)` starts delivery to the target's
                      supervisor balance.
Phase 3: Confirm       The operation settles, the output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the units are minted
                      back to the approver; the allocation is removed.
```

### 4.2 Permanent Release

A permanent release credits a target service's regular balance.

Units reach a regular balance by two routes: the DAO releases them to a named
service (§4.2.1), or a holder releases their own units (§4.2.2). A market sale
uses the second route: the DAO grants an adapter contract a budget (`grant`),
the adapter sells the units on the Hub, and the buyer releases them.

#### 4.2.1 Governance-initiated Release

Governance releases units from DAO custody.

```
Phase 1: Approve      `approve_release(target, amount, memo, ..)` records the
                      release allocation.
Phase 2: Execute      `execute_release(id)` starts delivery to the target's
                      regular balance.
Phase 3: Confirm       The operation settles, the output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the allocation moves to
                      Delivered.
                      Failed: JAM rejected the transfer; the units are minted
                      back to custody; the allocation returns to Approved.
```

#### 4.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service,
bypassing governance:

```
Phase 1: Redeem       `redeem(amount, target, memo)` starts delivery to
                      the target's regular balance.
Phase 2: Confirm       The operation settles, the output:
                      Confirmed: the credit sits on the target's regular
                      balance.
                      Failed: JAM rejected the transfer; the units are minted
                      back to the holder.
```

### 4.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reclaim      `reclaim` starts a return from the target's supervisor
                      balance back to the Parachain Service.
Phase 3: Confirm       The operation settles, the output:
                      Confirmed: `amount` is minted to the approver; a
                      fully returned lease is removed. Once the target
                      has no accepted lease, `unsupervise` may be called.
                      Failed: JAM rejected the transfer, for example because
                      the target would fall below its threshold balance; see
                      `Reclaim`.
```

Non-cooperative return (`Reclaiming`):

Supervision gives the pallet full power over the target, including cleaning
its state and ejecting it. The approver freezes and ejects; anyone can clean
up and forget. `cleanup_storage` and `forget_preimage` are bonded with a
deposit (§3.2). For a lease from a DAO adapter, governance may fund the
recovery as a treasury bounty:

```
Phase 1: Freeze       `freeze_target` stops the target from growing further.
Phase 2: Cleanup      `cleanup_storage` removes storage in pages.
                      Then one of:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; `unfreeze_target` and
                          `unsupervise` end the enforced cleanup, leaving
                          the target self-supervised.
                      (b) TERMINATE: continue below.
Phase 3: Forget       `forget_preimage` discards the preimage requests, the
                      code's among them, until the target holds none.
Phase 4: Eject        `eject_target` removes the target.
Phase 5: Confirm       The eject settles; the amounts of the target's leases
                      are minted to their approvers; the leases are removed.
```

- Storage keys are not recoverable from JAM state. They are tracked from the
  target's onboarding, or regenerated by replaying the target's blocks from
  its creation.
- The same holds for preimage requests; each needs a `forget_preimage`.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = the beneficiary account, §5.2); the
                      Parachain Service queues it in `incoming_transfers`.
Phase 2: Book         pallet-jamkb mints the amount:
                      Valid memo: to the beneficiary's Asset Hub account. 
                      Missing or malformed memo: nothing is minted; the
                      amount is recorded with its source (`Excess`).
```

---

## 5. Message Protocol

### 5.1 Operations, Correlation

An `OperationId` is unique and never reused.
An operation is resolved when parachain-system reports its outcome and its log
entries (§7.6).

A `Delivery`, `Increase`, `Redemption`, `Reclaim`, `Refund` or `Move` sends one
`TransferOut` carrying a `MessageId`. A `TransferFailed`
entry in its outcome means it failed.

A recovery operation's messages carry no `MessageId`. Their entries
(`ServiceKeyNotFound`, `ServiceForgetNotRequested`, `ServiceEjectFailed`,
`ServiceSupervisorFailed`, `ServiceEjected`, `ForgetAgainAt`) carry the
service id. parachain-system reports them as the outcome of the recovery
operation on that service (§3.2).

### 5.2 Memo Requirements

JAM transfer memos are 128 octets. A redemption, a release and a refund
carry the memo their caller supplies, in a format the destination service
defines. Every deferred `TransferOut` carries `MEMO_GAS`. A service that
receives JAMKB from Asset Hub keeps its `min_memo_gas` at or below
`MEMO_GAS`; a transfer to a service above it is refused and undone. A
voluntary return carries the beneficiary account as its first 32 bytes, the
rest zero; an H160 beneficiary is encoded as in §3.2.1. A memo of all zero
bytes is missing.

---

## 6. References

- [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926):
  burn of all DAO proceeds from JAMKB; no grants, gifts, or below-market loans
- [JAM Gray Paper](https://graypaper.com): formal JAM specification (Gavin
  Wood); this document follows revision
  [`f01d06d`](https://github.com/gavofyork/graypaper/tree/f01d06d5ca6aa10a4a123e185f57f18df908eb12),
  which includes the unreleased junior services change
- [Parachain Service on JAM](https://github.com/paritytech/polkadot-sdk/pull/11883)
- [DOT DAO and the need for $JAMKB](https://medium.com/polkadot-network/dot-dao-and-the-need-for-jamkb-a069e72e9728):
  Gavin Wood
- [DOT DAOism under JAM: An Island Story](https://medium.com/polkadot-network/dot-daoism-under-jam-an-island-story-efe0d02ee084):
  Gavin Wood

---

## 7. Open Issues

1. **Launch distribution.** At launch the existing parachains receive the JAMKB
   they need; the distribution contract comes later. Referendum 1926 allows
   only selling JAMKB for DOT at market price, with no exception for system
   chains, so a grant to any parachain, system chains included, conflicts with
   it. How the launch allocation is paid for is open.
2. **Storage footprint cleanup.** The pallet does not know the value length of
   a deleted key, so it cannot reclaim what a cleanup page freed. The freeing
   and the reclaim must also be atomic: with several leases on one target,
   their reclaims race for the freed funds, and the lease the cleanup was done
   for should be the one to get them back.
3. **Forced head.** Coretime's `ParachainSetHead` (Parachain Service design
   §6.3) moves Asset Hub's head, and its state, without accumulating
   anything. JAM's state does not follow, so the accounting diverges.
4. **Parachain Service supervisor.** A `Move` needs the Parachain Service to
   be its own supervisor. The Parachain Service design has to guarantee it.
5. **parachain-system interface.** parachain-system interface proposal is as
   follows:

   ```rust
   /// Each sender has its own budget share.
   type SenderId = u8;
   /// The id a `TransferOut` or `CreateService` carries, echoed by the
   /// Parachain Service in `TransferFailed` and `ServiceCreation`. Unique
   /// and never reused.
   type MessageId = u64;
   /// One atomic batch of upward messages.
   type OperationId = u64;

   /// Implemented by parachain-system: the current timeslot (§3.4), as a
   /// `BlockNumberProvider<BlockNumber = Timeslot>`.
   type TimeslotProvider;

   /// Implemented by parachain-system. An operation's messages are sent in
   /// one block, in the order the sender queued them. An operation is in
   /// flight from `send` until `on_outcome`.
   trait JamMessageSender {
       /// Allocates the id for one `TransferOut` or `CreateService`. The
       /// sender puts it in the message before `send`.
       fn next_message_id() -> MessageId;
       /// Queues `messages` as one operation and returns its id. A message
       /// is identified by its `MessageId`, or else by (kind, target).
       /// `Conflict`: a (kind, target) name is in flight for another
       /// operation, of any sender. `TooBig`: the batch can never fit one
       /// block. `QueueFull`: no room now. On `Err` nothing is queued.
       fn send(sender: SenderId, messages: Vec<UpwardMessage>)
           -> Result<OperationId, MessageSendError>;
   }

   /// Implemented by each sender. Called by parachain-system.
   trait OnJamOutcome {
       /// Operation `id` of `sender` was accumulated. Once per operation,
       /// in send order. `accumulated_at` is the timeslot it was accumulated
       /// at; `entries` are its `parachain_log` entries, at most
       /// one per message. A sender gets only the outcomes of its own
       /// operations. It cannot fail and charges what it writes to `meter`:
       /// one bounded record per outcome, which parachain-system budgets in
       /// its inherent for every outcome it delivers.
       fn on_outcome(
           sender: SenderId,
           id: OperationId,
           accumulated_at: Timeslot,
           entries: &[AccumulateLog],
           meter: &mut WeightMeter,
       );
   }

   /// Implemented by `pallet-jamkb`. Called by parachain-system.
   trait OnIncomingTransfers {
       /// How many buckets `on_incoming_transfers` takes this block. Asked
       /// before any bucket is read. 0 until attested and while paused.
       fn capacity() -> u32;
       /// One whole `incoming_transfers` bucket, in queue order; at most
       /// `capacity()` buckets per block. The pallet books the bucket's
       /// transfers to the regular balance in the call and records one
       /// `Move` for the rest (§3.2), all or nothing, and charges it to
       /// `meter`. `Ok`: the bucket is cleaned up and not delivered again.
       /// `Err`: its `Move` cannot be recorded; it is not booked.
       /// Parachain-system rolls the call back, delivers no more buckets
       /// this block, cleans up only the ones before it, and delivers it
       /// again in a later block.
       fn on_incoming_transfers(
           transfers: IncomingTransfers,
           meter: &mut WeightMeter,
       ) -> Result<(), ()>;
   }
   ```

6. **Transfer to Coretime.** The Coretime chain needs `pallet-assets` with a
   JAMKB asset, its XCM config trusting Asset Hub as the JAMKB reserve, and a
   call that returns units to Asset Hub after lowering the quota. The
   Coretime sovereign account on Asset Hub and each parachain sovereign
   account on Coretime need a DOT balance before the first deposit.
7. **Unbounded `Unclaimed`.** A return to an account that cannot receive the
   mint is booked in `Unclaimed` with no bound and no deposit, so a JAM
   service can add rows for the cost of the transfer. How to bound this is open.
