# JAMKB Management on Asset Hub

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Asset Hub Components](#3-asset-hub-components)
   - 3.1 [The JAMKB Asset](#31-the-jamkb-asset)
   - 3.2 [`pallet-jamkb`](#32-pallet-jamkb)
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

---

## 1. Overview

This document describes the architecture of JAMKB management on Asset Hub. JAMKB
is JAM's resource-access token for state footprint. A JAM service may keep as
much state as its balance covers. Asset Hub carries a 1:1 representation of the
token, where it is managed, sold and leased.

### Scope

This document covers:

- The flows: supervisor-managed allocation (lease, loan), permanent release and
  return of funds
- The components they use: the JAMKB asset, `pallet-jamkb`, the policy adapters

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

Initially all JAMKB sits on the Parachain Service balance. All the management
is done on Asset Hub. The DAO owns those funds and is responsible for their
distribution.

When a balance transfer from Asset Hub to a target JAM service is executed, the
pallet locks the requested amount on Asset Hub. On JAM the same amount moves
from the Parachain Service balance to the target JAM service.

The detailed flow below is a governance-executed permanent release (§4.2.1): one
deferred transfer from the Parachain Service balance to the target's regular
balance.

```
━━ Asset Hub block B — execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_release(target, amount)
   │  execute_release(id)
   ▼
pallet-jamkb
   │  holds the units, records the operation, and appends a TransferOut to
   │  PendingOperations
   ▼
pallet-parachain-system
   │  pulls PendingOperations, calls the Parachain Service's
   │  send_upward_message(TransferOut{..})

━━ B's work report — Accumulate, on-chain on JAM ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Parachain Service
   │  transfer its balance → target's regular balance
   │  on failure it writes TransferFailed { id, error } to Asset Hub's
   │  parachain_log
   │  records B's header as Asset Hub's para head

━━ Asset Hub block C — a later block, its lookup-anchor at or past B's accumulation ━

pallet-parachain-system
   │  delivers the verified validation inputs (para head, complete
   │  parachain_log, incoming_transfers)
   ▼
pallet-jamkb
   │  stores the validation inputs; matches the failure entries from
   │  parachain_log against its Submitted operations and marks them Failed

━━ Any later Asset Hub block ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pallet-jamkb
   │  settle(op_id), permissionless, updates the operation state:
   │  - para head hash is B or a descendant of B, no TransferFailed for the
   │    operation's id → Confirmed, the units move to the `released` account
   │  - a TransferFailed for the operation's id → Failed
   │  - para head not yet at B → the operation stays pending
```

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. This asset is managed by `pallet-jamkb`. It
holds the four privileged roles (Owner, Issuer, Admin, Freezer), assigned to it
at initialization.

The full JAMKB cap is minted into the pallet's custody account (§3.2), so that
Asset Hub holds a 1:1 representation of the JAM-side balance. The mint is a
one-time governance-executed runtime call on the Asset Hub.

Parachain state footprint is backed on the Coretime chain. JAMKB is teleported
there, and the Coretime chain backs each parachain's footprint against what it
holds. Teleported units leave Asset Hub's spendable supply and park in the XCM
checking account; the cap does not move.

### 3.2 `pallet-jamkb`

The pallet keeps the JAMKB in four keyless accounts derived from its
`PalletId`. A unit changes account by a transfer between them.

| Account     | Derivation                             | Balance                         |
|-------------|----------------------------------------|---------------------------------|
| `custody`   | `into_account_truncating()`            | DAO undistributed units         |
| `released`  | `into_sub_account_truncating(b"rlsd")` | permanent releases              |
| `custodial` | `into_sub_account_truncating(b"cstd")` | returns awaiting `claim` (§4.4) |
| `excess`    | `into_sub_account_truncating(b"xces")` | unattributed inflows (§4.4)     |

A FRAME pallet that manages the JAMKB asset and executes transfer operations
against the Parachain Service. Before any action like a permanent
release or a lease is triggered, the tokens are locked in place: a hold is
placed on them, and only then does the transfer execute on the JAM side. On
confirmation the tokens of a lease stay held; the tokens of a permanent release
move into the `released` account. On failure the pallet undoes what the call
did.

Every transfer operation is asynchronous and its outcome, Confirmed or Failed, is resolved on Asset Hub by
`settle`.

The pallet also supports voluntary return (§4.4): a JAM service sends its
funds back to the Parachain Service balance, setting an Asset Hub beneficiary
in the transfer memo. The pallet records the returned amount as claimable, and
the beneficiary claims the units back to its Asset Hub account.

The pallet starts inactive. Governance verifies, against public JAM state at a
defined block, that the Parachain Service holds `CAP`. It records the result
with `attest(anchor)`; the pallet stores the anchor. Until attested, every call
except `initialize` and `attest` is rejected.

```rust
/// A JAM block reference. The attestation sets the block whose state the
/// balances were checked against. The pallet records it and does not
/// verify it.
struct Anchor {
    timeslot: Timeslot,
    header_hash: Hash,
}

/// The fixed JAMKB supply, minted at `initialize`.
const CAP: Balance;

type AllocationId = u64;
type OperationId = u64;

/// Added to the lease end before `reclaim` opens (`reclaim`, §3.2).
const GRACE_PERIOD: BlockNumber;

/// The recovery deposit (§4.3): a hold on the caller's native balance under the
/// `RecoveryDeposit` hold reason on `pallet-balances`, `RECOVERY_DEPOSIT` per
/// operation.
const RECOVERY_DEPOSIT: Balance;
/// `RECOVERY_DEPOSIT_PER_KEY` per key of a `cleanup_storage` page.
const RECOVERY_DEPOSIT_PER_KEY: Balance;

/// A raw storage key of a supervised service (required for `cleanup_storage`,
/// §4.3), and the keys in one page.
const MAX_KEY_LEN: u32;
const MAX_KEYS_PER_PAGE: u32;
type Key = BoundedVec<u8, MAX_KEY_LEN>;

/// The Asset Hub account associated with a JAM service.
type ServiceAccounts = Map<ServiceId, AccountId>;

/// Awaiting `claim`, per beneficiary (§4.4).
type Custodial = Map<AccountId, Balance>;

/// Unattributed inflows, per source service (§4.4).
type Excess = Map<ServiceId, Balance>;

/// The frozen lease targets (§4.3).
type FrozenTargets = Set<ServiceId>;

/// One approved allocation.
struct Allocation {
    id: AllocationId,
    mode: AllocationMode,             // Lease { duration } | Permanent
    target: ServiceId,
    amount: Balance,
    state: AllocationState,
    delivered_at: Option<BlockNumber>,
                                      // the block the delivery was sent in.
                                      // A lease ends at
                                      // `delivered_at + duration` (§4.1)
    approver: PalletsOrigin,          // a signed origin (an account or a
                                      // contract) or governance
    valid_from: BlockNumber,          // execute lease and release are rejected before this block
    expires_at: BlockNumber,          // execute is rejected from this block
                                      // on
}

enum AllocationMode {
    Lease { duration: BlockNumber },
    Permanent,
}

enum AllocationState {
    Approved,
    Delivering,                       // credit in flight (lease or release);
                                      // on Failed, back to Approved
    Delivered,                        // the delivery settled Confirmed. For a
                                      // release it is terminal: the DAO no
                                      // longer controls the units. For a
                                      // lease, reclaim may follow
    Reclaiming,                       // entered by a Failed `Reclaim`; Lease
                                      // only
    Closed,                           // cancelled, returned, ejected or
                                      // closed by `close_lease`
}

/// One cross-system operation between Asset Hub and JAM.
struct Operation {
    id: OperationId,                  // unique operation Id (§5.1)
    payload: OperationPayload,
    state: OperationState,
    submitted_at: Option<BlockNumber>,
                                      // the block the messages were sent in;
                                      // unset until then
}

/// The operation's data payload.
enum OperationPayload {
    /// A lease or a governance permanent release (§4.1, §4.2.1).
    /// One `TransferOut` crediting the allocation's target; deferred when the
    /// allocation's mode is `Permanent`.
    ///
    /// Confirmed: the allocation moves to `Delivered` and `delivered_at` is
    /// set to the operation's `submitted_at`; for a release, the held units
    /// move to the `released` account.
    Delivery {
        allocation: AllocationId,
        amount: Balance,              // the units locked for the transfer
    },
    /// A lease increase (`increase_lease`).
    /// One plain `TransferOut` crediting the target's supervisor balance.
    ///
    /// Confirmed: the allocation's amount rises by `additional`; the lease
    /// stays `Delivered`.
    Increase {
        allocation: AllocationId,
        additional: Balance,
    },
    /// A redemption of holder funds (§4.2.2).
    /// One deferred `TransferOut` crediting `target`'s regular balance.
    ///
    /// Confirmed: the held units move from `holder` to the `released` account.
    Redemption {
        holder: AccountId,            // the redeemer; the hold sits on it
        target: ServiceId,
        amount: Balance,              // the units locked for the transfer
    },
    /// A lease return (`reclaim`, §4.3).
    /// One plain `TransferOut` debiting the lease target's supervisor balance
    /// and crediting the Parachain Service.
    ///
    /// Confirmed: `amount` is released from the hold; a fully returned lease
    /// moves to `Closed`.
    /// Failed: a `Delivered` lease past the lease end and `GRACE_PERIOD` moves
    /// to `Reclaiming`.
    Reclaim {
        allocation: AllocationId,
        amount: Balance,
    },
    /// A freeze (`freeze_target`, §4.3).
    /// One message that stops the target from taking more state footprint.
    ///
    /// Confirmed: the target is recorded in `FrozenTargets`.
    Freeze {
        target: ServiceId,
    },
    /// An unfreeze (`unfreeze_target`, §4.3).
    /// One message that reverses the freeze.
    ///
    /// Confirmed: the target is removed from `FrozenTargets`.
    Unfreeze {
        target: ServiceId,
    },
    /// A cleanup page (`cleanup_storage`, §4.3).
    /// One `RemoveServiceStorage { service: target, key }` per key in `keys`.
    ///
    /// Confirmed: the deposit is released.
    Cleanup {
        target: ServiceId,
        keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>,
        deposit: (AccountId, Balance),
    },
    /// A preimage release (`forget_preimage`, §4.3).
    /// One `Forget { target: Service(target), hash, len }`.
    ///
    /// Confirmed: the deposit is released.
    Forget {
        target: ServiceId,
        hash: Hash,
        len: u32,
        deposit: (AccountId, Balance),
    },
    /// An ejection (`eject_target`, §4.3).
    /// One `EjectService { service: target }`; it fails `NotEmpty` until the
    /// target's storage and requests are empty.
    ///
    /// Confirmed: every lease on the target moves to `Closed` and its hold is
    /// released; the target's `FrozenTargets` and `ServiceAccounts` entries
    /// are removed; the deposit is released. The amount above `leased` is booked as
    /// excess (§4.4).
    Eject {
        target: ServiceId,
        leased: Balance,              // the sum of the target's leases'
                                      // amounts at the call (§4.4)
        deposit: (AccountId, Balance),
    },
    /// A release from supervision (`unsupervise`, §4.3).
    /// One `SetServiceSupervisor { service: target, new_supervisor: target }`.
    ///
    /// Confirmed and Failed: no further state change.
    Unsupervise {
        target: ServiceId,
    },
    /// A refund of an incoming transfer to its source (`dispose_excess`).
    /// One deferred `TransferOut` crediting `source`'s regular balance.
    ///
    /// Confirmed: the held units move from the `excess` account to the
    /// `released` account.
    Refund {
        source: ServiceId,
        amount: Balance,
    },
}

enum OperationState {
    Requested,          // recorded and queued
    Submitted,          // sent: parachain-system pulled it from the queue (§3.4)
    Confirmed,
    Failed,
}

/// The failure carried by `OperationFailed`: one variant per failure entry
/// class, each wrapping that entry's `error` field (Parachain Service design
/// §3.1).
enum FailureReason {
    Transfer(TransferError),            // a TransferFailed entry (§5.1)
    Store(ServiceStoreError),           // a ServiceStoreFailed entry
    Eject(ServiceEjectError),           // a ServiceEjectFailed entry
    Supervisor(ServiceSupervisorError), // a ServiceSupervisorFailed entry
}

/// The booking class of an uncredited return (§4.4).
enum ReturnClass {
    Custodial,          // owner known, reserved for its claimant
    Excess,             // owner unknown, disposed by governance
}
```

Aggregates maintained for reporting:

```rust
struct Totals {
    leased: Balance,           // the sum of `Leased` holds
    releasing: Balance,        // the sum of `Releasing` holds
}
```

Typed events:

```rust
enum Event {
    /// The genesis attestation is recorded.
    Attested { anchor: Anchor },
    ReleaseApproved { id: AllocationId, target: ServiceId, amount: Balance },
    LeaseOffered { id: AllocationId, target: ServiceId, amount: Balance,
                   duration: BlockNumber },
    AllocationCancelled { id: AllocationId },
    LeaseClosed { id: AllocationId, amount: Balance },
    LeaseIncreased { id: AllocationId, additional: Balance },
    LeaseExtended { id: AllocationId, duration: BlockNumber },
    /// `allocation` is `None` for a redemption, a refund or a recovery step.
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// An amount was booked (§4.4): custodial for a named beneficiary, or
    /// excess.
    ReturnBooked { source: ServiceId, amount: Balance, class: ReturnClass },
    /// A beneficiary's custodial balance was claimed.
    Claimed { beneficiary: AccountId, amount: Balance },
    TargetFrozen { target: ServiceId },
    TargetUnfrozen { target: ServiceId },
    BudgetGranted { beneficiary: AccountId, amount: Balance },
    ExcessDisposed { source: ServiceId, amount: Balance },
    Paused,
    Resumed,
}
```

The pallet calls. Every call returns `DispatchResult`. New allocation and
operation ids are reported in the events.

```rust
// ── Setup and control ──────────────────────────────────────────────────────

/// Creates the JAMKB asset. Origin: governance (Root). Mints `CAP` into
/// custody and takes a provider reference on each pallet account.
fn initialize();

/// Records the genesis attestation against JAM state. Origin: governance
/// (Root). Enables operations.
fn attest(anchor: Anchor);

/// Sets and clears the pallet pause flag. Origin: governance (Root). While
/// set, a call that creates an allocation, records an operation or
/// distributes units from custody is rejected. Messages already queued are still processed.
fn pause();
fn resume();

// ── Distribution from custody ──────────────────────────────────────────────

/// Grants a budget to a policy adapter contract (§1). Origin: governance
/// (Root). Transfers `amount` of undistributed units from the pallet's
/// custody to `beneficiary` on Asset Hub.
fn grant(beneficiary: AccountId, amount: Balance);

// ── Permanent release ──────────────────────────────────────────────────────

/// Approves a permanent release to `target`. Origin: governance (Root).
/// Records `Allocation{mode: Permanent, state: Approved}` and holds `amount`
/// on custody (§3.3). Rejected when `target` is the Parachain Service.
fn approve_release(target: ServiceId, amount: Balance,
                   valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Executes an approved release. Origin: any signed account. Legal while
/// the allocation is `Approved`. Moves the allocation to `Delivering` and
/// records a `Delivery` operation.
fn execute_release(id: AllocationId);

/// Moves the caller's own units to `target`. Origin: any JAMKB holder. Holds
/// `amount` on the caller and records a `Redemption` operation with the
/// caller as `holder`. Rejected when `target` is the Parachain Service.
fn redeem(amount: Balance, target: ServiceId);

// ── Lease ──────────────────────────────────────────────────────────────────

/// Offers a lease of `amount` to `target` for `duration`. Origin: a token
/// holder or governance. Records `Allocation{mode: Lease, state: Approved}`
/// and holds `amount` on the approver's account, or on custody when the
/// approver is governance (§3.3). Rejected if the target has no service
/// account or is the Parachain Service.
fn offer_lease(target: ServiceId, amount: Balance, duration: BlockNumber,
               valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Accepts an offered lease. Origin: the target's service account
/// (`ServiceAccounts`). Legal while the allocation is `Approved`. Rejected
/// while the target has another lease within its term, two leases past their
/// term, or an unsettled `Eject` operation. Moves the allocation to
/// `Delivering` and records a `Delivery` operation.
fn accept_lease(id: AllocationId);

/// Raises a lease's `amount` by `additional` funds. Origin: the lease's approver.
/// Legal while the lease is `Delivered` and within its term. Holds
/// `additional` on the approver's account and records an `Increase`
/// operation. The lease end does not change.
fn increase_lease(id: AllocationId, additional: Balance);

/// Sets the lease's `duration`. Origin: the lease's approver. Legal while the
/// lease is `Delivered` and within its term. Rejected unless `duration` is
/// greater than the current one.
fn extend_lease(id: AllocationId, duration: BlockNumber);

/// Takes `amount` of the leased units back from the target. Origin: the
/// lease's approver past the lease end and `GRACE_PERIOD`, or the target's
/// service account at any time. Legal from `Delivered` and from
/// `Reclaiming`. Records a `Reclaim` operation for `amount`, capped at the
/// allocation's remaining amount.
fn reclaim(id: AllocationId, amount: Balance);

/// Ends a lease that was not fully returned. Origin: the lease's approver or
/// governance. Legal while the lease is `Reclaiming`. Rejected while the
/// lease has an unsettled operation (§5.1). The held units move to the
/// `released` account and the lease moves to `Closed`.
fn close_lease(id: AllocationId);

// ── Recovery: a lease target that does not cooperate ───────────────────────

/// Stops the target from taking more state footprint. Origin: the lease's
/// approver, the target's service account, or governance. Legal while the
/// target is in recovery and not recorded frozen. Records a `Freeze`
/// operation.
fn freeze_target(target: ServiceId);

/// Reverses the freeze. Origin: the lease's approver, the target's service account or
/// governance. Legal while the target is recorded frozen and not in
/// recovery. Records an `Unfreeze` operation.
fn unfreeze_target(target: ServiceId);

/// Deletes one bounded page of keys from the target's storage. Origin: the
/// lease's approver, the target's service account, or governance, with
/// `RECOVERY_DEPOSIT` plus `RECOVERY_DEPOSIT_PER_KEY` per key. Legal while
/// the target is in recovery. Records a `Cleanup` operation.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Releases a solicited preimage of the target. Origin: the lease's approver,
/// the target's service account, or governance, with `RECOVERY_DEPOSIT`.
/// Legal while the target is in recovery and recorded frozen. A provided
/// preimage is
/// expunged by a second call more than `C_expungeperiod` timeslots after the
/// first (Parachain Service design §6.1). Rejected for the target's
/// code hash except from governance
/// Records a `Forget` operation.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Destroys the emptied target, crediting its balances to the Parachain
/// Service. Origin: the target's service account, or
/// governance, with `RECOVERY_DEPOSIT`. Legal while the target is in
/// recovery. Rejected while a lease on the target has an unsettled operation
/// (§5.1). Records an `Eject` operation
/// with `leased` = the sum of the target's leases' amounts.
fn eject_target(target: ServiceId);

/// Releases the supervised target to itself. Origin: the target's service
/// account or governance. Rejected while the target is recorded frozen, or
/// has a lease that is not `Closed`. Records an `Unsupervise` operation.
fn unsupervise(target: ServiceId);

// ── Returns and disposal ───────────────────────────────────────────────────

/// Claims the voluntarily returned balance to `beneficiary`'s account (§4.4).
/// Origin: any signed account. Moves the beneficiary's balance from the
/// `custodial` account to the `beneficiary` account..
fn claim(beneficiary: AccountId);

/// Disposes an excess amount per service. Origin: governance (Root).
/// With `refund = true` it creates a `Refund` operation sending the tokens
/// back to the `source` service's regular balance; with `refund = false` it
/// moves the tokens from the `excess` account to custody.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool);

// ── Shared by every flow ───────────────────────────────────────────────────

/// Cancels an allocation in `Approved`. Origin: the allocation's approver.
/// The hold is released and the allocation moves to `Closed`.
/// A `Delivering` allocation cannot be cancelled.
fn cancel_allocation(id: AllocationId);

/// Updates the operation state to Confirmed or Failed. Origin: any signed
/// account. On Failed the call that recorded the operation is undone and its
/// recovery deposit, if any, is slashed.
fn settle(op_id: OperationId);
```

### 3.3 pallet-assets access

`pallet-assets` holds JAMKB. The pallet holds the four roles and administers the
asset through the runtime-internal fungibles traits: `transfer`, `hold`,
`release`, `transfer_on_hold` and `mint`. No administration precompile is
needed. Asset-wide status changes need no role: governance uses
`force_asset_status` (`ForceOrigin`, Root on Asset Hub). Policy adapters move
their budgets through the existing ERC20 precompile.

**Hold rules.** `pallet-jamkb` locks units with a hold before it sends a
transfer. The runtime sets `pallet-assets-holder` as the asset's `Holder`; it
provides the hold traits. The hold reasons are declared as:

```rust
/// The hold classes.
enum HoldReason {
    /// Units backing a lease.
    #[codec(index = 0)]
    Leased,
    /// Units backing outstanding permanent releases.
    #[codec(index = 1)]
    Releasing,
}
```

### 3.4 The Generic AH→JAM Transport

The pallet contains no code that reaches the Parachain Service directly, so it
is layered on a generic transport mechanism. The planned
[cumulus-on-jam](https://github.com/paritytech/polkadot-sdk/pull/12714) §11 `validate_block`
rework is expected to provide it, probably by extending `parachain-system` into
a generic AH→JAM transport pallet. The generic pallet would own the outbound
upward messages, sent through `send_upward_message` inside
`jam_validate_block`, and the inherent that delivers the validation inputs.

#### The parachain-system pallet requirements

The design of that pallet is outside the scope of this document. The
requirements below are what the pallet needs from it to operate.

The Parachain Service restricts `TransferOut` and its sibling upward messages:
they are accepted only from the Asset Hub parachain. Such `UpwardMessage`
variants have no public push API in the parachain-system pallet. The generic
pallet pulls each restricted message variant from one provider named in the
runtime `Config`, following the `XcmpMessageSource` pattern.

The generic pallet exposes the inherent data (`parachain_log` entries,
`incoming_transfers`) to pallets as validation inputs. `pallet-jamkb` books
each `incoming_transfers` entry once, in bucket order, from the `released`
account (§4.4), and sends `CleanUpBucketsUpTo` for all booked buckets but the
last. An entry larger than the `released` account is not booked until the
account covers it.

#### What the pallet owns

`pallet-jamkb` appends each operation's messages to `PendingOperations`, the
per-block send queue. The parachain-system pallet takes the queue through the
source trait and calls `send_upward_message` for each message; the operation
moves to `Submitted`. The runtime `Config` sets `pallet-jamkb` as the only
provider of the message variants it sends.

The parachain-system pallet checks the inherent `(anchor, proof, para head,
parachain_log, incoming_transfers)` against the state root after the anchor
block. It only provides the data, so `pallet-jamkb` has to handle it: it takes
the failure entries, finds the `Submitted` operations they name, and marks them
`Failed`. `settle(op_id)` then releases the holds, updates the allocation and
drops the record. The 64 KiB log cap bounds this work.

One risk remains: a `TransferFailed` entry can be overwritten in `parachain_log`
(its 64 KiB cap) before Asset Hub has read it. Asset Hub and JAM state then
disagree: the funds were never transferred on JAM, but the units stay locked on
Asset Hub.

---

## 4. Allocation Protocols

### 4.1 Lease (Supervisor-Managed Allocation)

A lease is a token transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor, and
the target has a service account (§3.2).

```
Phase 1: Offer        Any token holder or governance calls
                      `offer_lease(target, amount, duration, ..)`.
                      `amount` is held on the approver's account.
Phase 2: Accept       The target's service account calls `accept_lease(id)`
                      (§3.2). The pallet queues a TransferOut crediting the
                      target's supervisor balance (§3.4).
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§3.2).
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the units stay held, backing the lease;
                      the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the allocation is
                      back to Approved; the hold stays.
```

### 4.2 Permanent Release

A permanent release is a token transfer to the target service's regular balance.

Units reach a regular balance by two routes: the DAO releases them to a named
service (§4.2.1), or a holder releases their own units (§4.2.2). A market sale
uses the second route: the DAO grants an adapter a budget (`grant`, §3.2), the
adapter sells the units on the Hub, and the buyer releases them.

#### 4.2.1 Governance-initiated Release

Governance releases units from DAO custody.

```
Phase 1: Approve      Governance calls `approve_release(target, amount, ..)`
                      (§3.2); `amount` is held on custody.
Phase 2: Execute      Any signed account calls `execute_release(id)` (§3.2).
                      The pallet queues a TransferOut crediting the target's
                      regular balance (§3.4).
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§3.2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units move to the
                      `released` account; the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the allocation is
                      back to Approved; the hold stays.
```

#### 4.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service,
bypassing governance:

```
Phase 1: Redeem       Holder calls `redeem(amount, target)` (§3.2). The pallet
                      places a hold on the holder's units (§3.3) and
                      queues a TransferOut crediting the target's regular
                      balance (§3.4).
Phase 2: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 3: Confirm      Any party can call `settle(op_id)` on the pallet (§3.2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance; the units move to the `released` account.
                      Failed: JAM rejected the transfer; the hold is released.
```

### 4.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reclaim      The approver or the target's service account calls
                      `reclaim` (§3.2); the pallet queues a TransferOut
                      debiting the target's supervisor balance by the
                      requested amount (§3.4). A partial amount is legal.
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4). It fails if, after the
                      debit, balance + supervisor_balance < the threshold
                      balance.
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§3.2).
                      The output:
                      Confirmed: `amount` is released from the hold; a
                      fully returned lease moves to Closed; once every lease
                      on the target is Closed, unsupervise(target) may be
                      called.
                      Failed: JAM rejected the transfer; the lease moves to
                      Reclaiming (`Reclaim`, §3.2).
```

Full return, non-cooperative. Entered when a reclaim has failed, leaving the
lease `Reclaiming`. A target is in recovery.

Supervision gives the pallet full power over the target, including cleaning
its state and ejecting it. The approver, the target's service account or
governance runs the recovery; `cleanup_storage`, `forget_preimage` and
`eject_target` are bonded with a deposit (§3.2). For a governance-approved
lease, governance may fund the work as a treasury bounty:

```
Phase 1: Freeze       The approver submits `freeze_target` (§3.2).
Phase 2: Cleanup      The approver submits `cleanup_storage` pages (§3.2), and
                      `forget_preimage` for every preimage except the
                      target's code preimage.
                      The exit fork:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; the service account or
                          governance calls `unfreeze_target` (§3.2);
                          unsupervise(target) ends the enforced cleanup,
                          leaving the target self-supervised.
                      (b) TERMINATE: continue below.
Phase 3: Forget       The approver or governance discards the code preimage:
                      `forget_preimage` (§3.2).
Phase 4: Eject        The approver submits `eject_target` (§3.2).
Phase 5: Confirm      The eject settles by a `settle(op_id)` call on the
                      pallet (§3.2); the holds of the target's leases are
                      released; the leases move to Closed.
```

- Storage keys are not recoverable from JAM state. They are tracked from the
  target's onboarding, or regenerated by replaying the target's blocks from
  its creation, so a cleanup can always be completed.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = the beneficiary account, §5.2); the
                      Parachain Service queues it in `incoming_transfers`.
Phase 2: Claim        The pallet books the entry (§3.4). The output:
                      Valid memo (§5.2): the amount is booked as custodial for
                      the named Asset Hub account (`Custodial`, §3.2); the
                      beneficiary collects it with the permissionless `claim`
                      (§3.2).
                      Missing or malformed memo: the amount is booked as
                      excess (`Excess`, §3.2), recorded with its source; a
                      refund to the source is a governance disposition
                      (`dispose_excess`, §3.2).
```

---

## 5. Message Protocol

### 5.1 Operations, Correlation

An `OperationId` is unique and never reused. A failed operation is terminal;
a retry creates a new operation with a new id.

A `Delivery`, `Increase`, `Redemption`, `Reclaim` or `Refund` correlates by
id: its `TransferOut` carries the `OperationId` as its `id` field, and a
`TransferFailed { id }` entry points directly at the failed operation.

A recovery operation has no id in its message. Its failure entry
(`ServiceStoreFailed`, `ServiceEjectFailed`, `ServiceSupervisorFailed`,
`ServiceCodeFailed`) names the service, so it correlates by target and entry
class. The pallet keeps at most one unsettled operation per allocation and at
most one unsettled recovery operation per target.

### 5.2 Memo Requirements

JAM transfer memos are 128 octets. A voluntary return carries the beneficiary
account in it. The exact layout is to be defined.

---

## 6. References

- [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926):
  burn of all DAO proceeds from JAMKB; no grants, gifts, or below-market loans
- [JAM Gray Paper](https://graypaper.com):
  formal JAM specification (Gavin Wood)
- [Parachain Service on JAM](https://github.com/paritytech/polkadot-sdk/pull/11883)
- [DOT DAO and the need for $JAMKB](https://medium.com/polkadot-network/dot-dao-and-the-need-for-jamkb-a069e72e9728):
  Gavin Wood
- [DOT DAOism under JAM: An Island Story](https://medium.com/polkadot-network/dot-daoism-under-jam-an-island-story-efe0d02ee084):
  Gavin Wood
