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
code. So JAMKB is represented on Asset Hub. There, the DAO can distribute it
by referendum or through contracts that automate distribution. The managing
pallet is exposed to contracts through a precompile, which also lets
them lease and redeem. That allows a secondary market to form around the
asset.

### Scope

This document covers:

- The flows: supervisor-managed allocation (lease), permanent release and
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

Initially all JAMKB sits on the Parachain Service balance. Asset Hub
manages distribution on behalf of the DAO.

Example flow for governance permanent release, from approval through settlement.

```
━━ Asset Hub block B: execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_release(target, amount): the units are held on custody
   ▼
Any signed account
   │  execute_release(id)
   ▼
pallet-jamkb
   │  records the operation and appends a TransferOut to the send queue
   ▼
pallet-parachain-system
   │  pulls the send queue, calls the Parachain Service's
   │  send_upward_message(TransferOut{..})

━━ B's work report: Accumulate, on-chain on JAM ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Parachain Service
   │  transfer its balance → target's regular balance
   │  on failure it writes TransferFailed { id, error } to Asset Hub's
   │  parachain_log
   │  records B's header as Asset Hub's para head

━━ Asset Hub block C: a later block, its lookup-anchor at or past B's accumulation ━

pallet-parachain-system
   │  delivers the verified validation inputs (para head, complete
   │  parachain_log, incoming_transfers)
   ▼
pallet-jamkb
   │  stores the para head; for each new failure entry from parachain_log,
   │  looks for the corresponding operation and, if matched, marks it Failed

━━ Any later Asset Hub block ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pallet-jamkb
   │  settle(op_id):
   │  - the operation is marked Failed → the allocation returns to
   │    Approved
   │  - the para head is B or a descendant of B → Confirmed, the units
   │    move to the `released` account
   │  - the para head is not yet at B → the operation stays Submitted
```

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. This asset is managed by `pallet-jamkb`. It
holds the four privileged roles (Owner, Issuer, Admin, Freezer), assigned to it
at initialization.

The full JAMKB cap is minted into the `pallet-jamkb` custody account (§3.2).
The mint is a one-time executed runtime call on the Asset Hub.

Parachain state footprint is backed on the Coretime chain. JAMKB is teleported
there, and the Coretime chain backs each parachain's footprint against what it
holds. Teleported units leave Asset Hub's spendable supply and park in the XCM
checking account.

### 3.2 `pallet-jamkb`

`pallet-jamkb` manages JAMKB on Asset Hub and executes asynchronous
cross-system operations through `pallet-parachain-system`.

It keeps JAMKB in four keyless accounts derived from its `PalletId`,
`py/jamkb`. Units move between these accounts via transfers.

| Account     | Derivation                             | Balance                         |
|-------------|----------------------------------------|---------------------------------|
| `custody`   | `into_account_truncating()`            | DAO undistributed units         |
| `released`  | `into_sub_account_truncating(b"rlsd")` | permanently released units      |
| `custodial` | `into_sub_account_truncating(b"cstd")` | returns awaiting `claim` (§4.4) |
| `excess`    | `into_sub_account_truncating(b"xces")` | unattributed inflows (§4.4)     |

Before any action like a permanent release or a lease is triggered, the units
are locked in place: a hold is placed on them, and only then does the transfer
execute on the JAM side. Every cross-system operation is asynchronous and is
finalized by `settle`. On confirmation the units of a lease stay held; the
units of a permanent release move into the `released` account. On failure the
pallet undoes what the call did.

To resolve operations, the pallet receives the validation inputs each block
(§3.4). It stores the para head and marks the operations named by the failure
entries of `parachain_log` as `Failed`.

For voluntary return (§4.4) it processes `incoming_transfers`. A valid
beneficiary memo credits `Custodial`; a missing or malformed memo credits
`Excess`.

The pallet starts inactive. Governance verifies, against public JAM state at a
defined block, that the Parachain Service holds `CAP`. It records the result
with `attest(anchor)`; the pallet stores the anchor. Until attested, every call
except `initialize` and `attest` is rejected.

```rust
/// The header hash of the JAM block whose state the balances were checked
/// against. The pallet records it and does not verify it.
type Anchor = Hash;

/// The fixed JAMKB supply, minted at `initialize`.
const CAP: Balance;

type AllocationId = u64;              // assigned from 1;
type OperationId = u64;               // assigned from 1;

/// A JAM service index.
type ServiceId = u32;

/// Added to the lease end before `reclaim` opens.
const GRACE_PERIOD: BlockNumber;

/// The cap on a target's accepted leases: those in `Delivering`, `Delivered`
/// or `Reclaiming`.
const MAX_LEASES_PER_TARGET: u32;

/// The recovery deposit (§4.3): a hold on the caller's native balance per
/// `cleanup_storage` page.
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
                                      // `delivered_at + duration`
    approver: PalletsOrigin,          // a signed origin (an account or a
                                      // contract) or governance
    valid_from: BlockNumber,          // `accept_lease` and `execute_release`
                                      // are rejected before this block
    expires_at: BlockNumber,          // `accept_lease` and `execute_release`
                                      // are rejected from this block on
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
    Reclaiming,                       // entered by a Failed `Reclaim`;
                                      // Lease only
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
    /// move from custody to the `released` account.
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
    /// Confirmed: `amount` is released from the hold and the allocation's
    /// amount falls by `amount`; a fully returned lease moves to `Closed`.
    /// Failed: a `Delivered` lease past the lease end and `GRACE_PERIOD` moves
    /// to `Reclaiming`; otherwise the lease keeps its state.
    Reclaim {
        allocation: AllocationId,
        amount: Balance,
    },
    /// A freeze (`freeze_target`, §4.3).
    /// One message that stops the target from taking more state footprint.
    /// The Parachain Service design does not provide it.
    ///
    /// Confirmed: the target is recorded in `FrozenTargets`.
    Freeze {
        target: ServiceId,
    },
    /// An unfreeze (`unfreeze_target`, §4.3).
    /// One message that reverses the freeze. The Parachain Service design
    /// does not provide it.
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
    /// Confirmed and Failed: no further state change.
    Forget {
        target: ServiceId,
        hash: Hash,
        len: u32,
    },
    /// An ejection (`eject_target`, §4.3).
    /// One `EjectService { service: target }`; it fails `NotEmpty` until the
    /// target's storage and requests are empty.
    ///
    /// Confirmed: every accepted lease on the target moves to `Closed` and
    /// its hold is released; the target's `FrozenTargets` and
    /// `ServiceAccounts` entries are removed. The amount above the leases is
    /// booked as excess (`Excess`).
    ///
    /// Needs the Parachain Service to report the balance it credits (§7).
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
    /// `allocation` is `None` for a redemption, a refund or a recovery operation.
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// An amount was booked (§4.4): custodial for `beneficiary`, or excess
    /// when `beneficiary` is `None`.
    ReturnBooked { source: ServiceId, amount: Balance, class: ReturnClass,
                   beneficiary: Option<AccountId> },
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

Every call returns `DispatchResult`. New allocation and operation ids are
reported in the events.

```rust
// ── Setup and control ──────────────────────────────────────────────────────

/// Creates the JAMKB asset. Origin: governance (Root). Mints `CAP` into
/// custody.
fn initialize();

/// Records the genesis attestation against JAM state. Origin: governance
/// (Root). Enables operations.
fn attest(anchor: Anchor);

/// Sets the pallet pause flag. Origin: governance (Root). While set, a call
/// that creates an allocation, records an operation or distributes units from
/// custody is rejected. Messages already queued are still processed.
fn pause();

/// Clears the pallet pause flag. Origin: governance (Root).
fn resume();

// ── Distribution from custody ──────────────────────────────────────────────

/// Grants a budget to a policy adapter contract (§1). Origin: governance
/// (Root). Transfers `amount` of undistributed units from the pallet's
/// custody to `beneficiary` on Asset Hub.
fn grant(beneficiary: AccountId, amount: Balance);

// ── Permanent release ──────────────────────────────────────────────────────

/// Approves a permanent release to `target`. Origin: governance (Root).
/// Records `Allocation{mode: Permanent, state: Approved}` and holds `amount`
/// on custody (§3.3). `valid_from` `None` means the current block. Rejected
/// when `target` is the Parachain Service.
fn approve_release(target: ServiceId, amount: Balance,
                   valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Executes an approved release. Origin: any signed account. Legal while
/// the allocation is `Approved`. Moves the allocation to `Delivering` and
/// records a `Delivery` operation.
fn execute_release(id: AllocationId);

/// Moves the caller's own units to `target`. Origin: any signed account. Holds
/// `amount` on the caller and records a `Redemption` operation with the
/// caller as `holder`. Rejected when `target` is the Parachain Service.
fn redeem(amount: Balance, target: ServiceId);

// ── Lease ──────────────────────────────────────────────────────────────────

/// Offers a lease of `amount` to `target` for `duration`. Origin: a token
/// holder or governance. Records `Allocation{mode: Lease, state: Approved}`
/// and holds `amount` on the approver's account, or on custody when the
/// approver is governance (§3.3). `valid_from` `None` means the current
/// block. Rejected if the target has no service account or is the Parachain
/// Service.
fn offer_lease(target: ServiceId, amount: Balance, duration: BlockNumber,
               valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Accepts an offered lease. Origin: the target's service account
/// (`ServiceAccounts`). Legal while the allocation is `Approved`. Rejected
/// while the target has `MAX_LEASES_PER_TARGET` accepted leases. Moves the
/// allocation to `Delivering` and records a `Delivery` operation.
fn accept_lease(id: AllocationId);

/// Raises a lease's `amount` by `additional` funds. Origin: the lease's approver.
/// Legal while the lease is `Delivered` and within its term. Raises the
/// lease's hold by `additional` and records an `Increase` operation. The
/// lease end does not change.
fn increase_lease(id: AllocationId, additional: Balance);

/// Sets the lease's `duration`. Origin: the lease's approver. Legal while the
/// lease is `Delivered` and within its term. Rejected unless `duration` is
/// greater than the current one.
fn extend_lease(id: AllocationId, duration: BlockNumber);

/// Takes `amount` of the leased units back from the target. Origin: any
/// signed account past the lease end and `GRACE_PERIOD`, or the target's
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

/// Stops the target from taking more state footprint. Origin: any signed
/// account. Legal while a lease on the target is `Reclaiming` and the target
/// is not recorded frozen. Records a `Freeze` operation.
fn freeze_target(target: ServiceId);

/// Reverses the freeze. Origin: any signed account. Legal while the target is
/// recorded frozen and no lease on it is `Reclaiming`. Records an `Unfreeze`
/// operation.
fn unfreeze_target(target: ServiceId);

/// Deletes one bounded page of keys from the target's storage. Origin: any
/// signed account, with `RECOVERY_DEPOSIT` plus `RECOVERY_DEPOSIT_PER_KEY`
/// per key. Legal while a lease on the target is `Reclaiming`. Records a
/// `Cleanup` operation.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Releases a solicited preimage of the target. Origin: the target's service
/// account or governance. Legal while a lease on the target is `Reclaiming`
/// and the target is recorded frozen. A provided preimage is expunged by a
/// second call more than `C_expungeperiod` timeslots after the first
/// (Parachain Service design §6.1). Records a `Forget` operation.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Destroys the emptied target, crediting its balances to the Parachain
/// Service. Origin: the target's service account or governance. Legal while
/// a lease on the target is `Reclaiming`. Rejected while a lease on the
/// target has an unsettled operation (§5.1). Records an `Eject` operation.
/// While the `Eject` is unsettled, `accept_lease`, `increase_lease`,
/// `extend_lease`, `reclaim` and `close_lease` on the target are rejected.
fn eject_target(target: ServiceId);

/// Releases the supervised target to itself. Origin: the target's service
/// account or governance. Rejected while the target is recorded frozen, or
/// has an accepted lease. Records an `Unsupervise` operation.
fn unsupervise(target: ServiceId);

// ── Returns and disposal ───────────────────────────────────────────────────

/// Claims the voluntarily returned balance to `beneficiary`'s account (§4.4).
/// Origin: any signed account. Moves the beneficiary's balance from the
/// `custodial` account to the `beneficiary` account.
fn claim(beneficiary: AccountId);

/// Disposes an excess amount per service. Origin: governance (Root). Lowers
/// `Excess[source]` by `amount`. With `refund = true` it holds `amount` on the
/// `excess` account and records a `Refund` operation sending the units back to
/// the `source` service's regular balance; with `refund = false` it moves the
/// units from the `excess` account to custody.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool);

// ── Shared by every flow ───────────────────────────────────────────────────

/// Cancels an allocation in `Approved`. Origin: the allocation's approver, or
/// any signed account from `expires_at` on. The hold is released and
/// the allocation moves to `Closed`. A `Delivering` allocation cannot be
/// cancelled.
fn cancel_allocation(id: AllocationId);

/// Resolves an operation. Origin: any signed account. Operation marked
/// `Failed`: the call that recorded it is undone and its recovery deposit, if
/// any, is slashed. Operation `Submitted`: moves to Confirmed once the block
/// it was sent in is accumulated on JAM, otherwise stays `Submitted`.
fn settle(op_id: OperationId);
```

#### 3.2.1 pallet-jamkb precompile

Contracts access the pallet through a precompile at a fixed address. It
exposes the calls and reads a contract needs. Each precompile function calls
the internal pallet method that holds the logic shared with the dispatchable
of the same name. The call origin is the account that calls the precompile.
The precompile returns the new allocation id, or the operation id that
`settle` resolves later. On failure the call reverts with the pallet error
name.

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

address constant JAMKB_ADDR = 0x0000000000000000000000000000000000000903;

interface IJamkb {
    enum AllocationState { Approved, Delivering, Delivered, Reclaiming, Closed }
    enum AllocationMode { Lease, Permanent }
    enum OperationKind {
        Delivery, Increase, Redemption, Reclaim, Freeze, Unfreeze, Cleanup,
        Forget, Eject, Unsupervise, Refund
    }
    enum OperationState { Requested, Submitted, Confirmed, Failed }

    struct Allocation {
        uint64 id;
        AllocationState state;
        AllocationMode mode;
        uint32 target;
        uint128 amount;
        address approver;      // address(0) for governance
        uint32 deliveredAt;    // 0: not delivered
        uint32 duration;       // 0 for Permanent
        uint32 validFrom;
        uint32 expiresAt;
    }

    struct Operation {
        uint64 id;
        OperationKind kind;
        OperationState state;
        uint64 allocationId;   // 0 when the operation has none
        uint32 target;         // the payload's target, else the allocation's
                               // target, else the refund's source
        uint128 amount;        // the payload's amount or additional, else 0
        uint32 submittedAt;    // 0: not yet sent
    }

    // -- Permanent release --
    function executeRelease(uint64 allocationId) external returns (uint64 opId);
    function redeem(uint128 amount, uint32 target) external returns (uint64 opId);

    // -- Lease --
    /// `validFrom` 0 means the current block.
    function offerLease(uint32 target, uint128 amount, uint32 duration,
                        uint32 validFrom, uint32 validFor)
        external returns (uint64 allocationId);
    function acceptLease(uint64 allocationId) external returns (uint64 opId);
    function increaseLease(uint64 allocationId, uint128 additional)
        external returns (uint64 opId);
    function extendLease(uint64 allocationId, uint32 duration) external;
    function reclaim(uint64 allocationId, uint128 amount) external returns (uint64 opId);
    function closeLease(uint64 allocationId) external;

    // -- Recovery --
    function freezeTarget(uint32 target) external returns (uint64 opId);
    function unfreezeTarget(uint32 target) external returns (uint64 opId);
    function cleanupStorage(uint32 target, bytes[] calldata keys)
        external returns (uint64 opId);
    function forgetPreimage(uint32 target, bytes32 hash, uint32 len)
        external returns (uint64 opId);
    function ejectTarget(uint32 target) external returns (uint64 opId);
    function unsupervise(uint32 target) external returns (uint64 opId);

    // -- Returns --
    function claim(address beneficiary) external returns (uint128 claimed);

    // -- Shared --
    function cancelAllocation(uint64 allocationId) external;
    /// Returns the operation's state after the call.
    function settle(uint64 opId) external returns (OperationState state);

    // -- Reads. allocation() and operation() revert for an unknown id. --
    function allocation(uint64 allocationId) external view returns (Allocation memory);
    function operation(uint64 opId) external view returns (Operation memory);
    function claimableOf(address beneficiary) external view returns (uint128);
    function serviceAccount(uint32 target) external view returns (address);
    function isFrozen(uint32 target) external view returns (bool);
    function leasesOf(uint32 target) external view returns (uint64[] memory allocationIds);
    function gracePeriod() external view returns (uint32);
    function maxLeasesPerTarget() external view returns (uint32);
    function recoveryDeposit() external view returns (uint128);
    function recoveryDepositPerKey() external view returns (uint128);
    function maxKeysPerPage() external view returns (uint32);
}
```

### 3.3 pallet-assets access

`pallet-assets` holds JAMKB. `pallet-jamkb` holds the four roles and
administers the asset through the runtime-internal fungibles traits:
`transfer`, `hold`, `release`, `transfer_on_hold` and `mint`. Policy adapters
move their JAMKB balances through the ERC20 precompile.

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

The pallet contains no code that reaches the Parachain Service directly. It
is layered on a generic transport mechanism. The planned
[cumulus-on-jam](https://github.com/paritytech/polkadot-sdk/pull/12714) §11 `validate_block`
rework is expected to provide it, probably by extending `parachain-system` into
a generic AH→JAM transport pallet. The generic pallet would own the outbound
upward messages, sent through `send_upward_message` inside
`jam_validate_block`, and the inherent that delivers the validation inputs.

#### The parachain-system pallet requirements

The design of that pallet is outside the scope of this document. The
requirements below are what the pallet needs from it to operate.

The generic pallet checks the inherent `(lookup-anchor, proof, para head,
parachain_log, incoming_transfers)` against the state root after the
lookup-anchor block and exposes the para head, `parachain_log` and
`incoming_transfers` to pallets as validation inputs.

It also accepts the messages `pallet-jamkb` queues and sends them through
`send_upward_message`.

---

## 4. Allocation Protocols

This section shows the end-to-end protocol only. Origins, validity rules and
immediate local effects are defined in the call reference in §3.2.
Unless stated otherwise, once a call records an operation, the generic
transport in §3.4 later sends the queued message and `settle` resolves the
outcome.

### 4.1 Lease (Supervisor-Managed Allocation)

A lease is a token transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor, and
the target has a service account (§3.2).

```
Phase 1: Offer        `offer_lease(target, amount, duration, ..)` records the
                      lease offer.
Phase 2: Accept       `accept_lease(id)` starts delivery to the target's
                      supervisor balance.
Phase 3: Confirm      `settle(op_id)` resolves the operation.
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the units stay held, backing the lease;
                      the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the allocation
                      returns to Approved.
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
Phase 1: Approve      `approve_release(target, amount, ..)` records the
                      release allocation.
Phase 2: Execute      `execute_release(id)` starts delivery to the target's
                      regular balance.
Phase 3: Confirm      `settle(op_id)` resolves the operation.
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units move to the
                      `released` account; the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the allocation
                      returns to Approved.
```

#### 4.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service,
bypassing governance:

```
Phase 1: Redeem       `redeem(amount, target)` starts delivery to the
                      target's regular balance.
Phase 2: Confirm      `settle(op_id)` resolves the operation.
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance; the units move to the `released` account.
                      Failed: JAM rejected the transfer; the pallet undoes the
                      redemption.
```

### 4.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reclaim      `reclaim` starts a return from the target's supervisor
                      balance back to the Parachain Service. The return may
                      be partial.
Phase 3: Confirm      `settle(op_id)` resolves the operation.
                      The output:
                      Confirmed: `amount` is released from the hold; a
                      fully returned lease moves to Closed. Once the target
                      has no accepted lease, `unsupervise` may be called.
                      Failed: JAM rejected the transfer, for example because
                      the target would fall below its threshold balance; see
                      `Reclaim`.
```

Non-cooperative return (`Reclaiming`):

Supervision gives the pallet full power over the target, including cleaning
its state and ejecting it. `cleanup_storage` is bonded with a deposit (§3.2).
For a governance-approved lease, governance may fund the work as a treasury
bounty:

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
Phase 3: Forget       `forget_preimage` discards the code preimage.
Phase 4: Eject        `eject_target` removes the target.
Phase 5: Confirm      `settle(op_id)` resolves the eject; the holds of the
                      target's leases are released; the leases move to Closed.
```

- Storage keys are not recoverable from JAM state. They are tracked from the
  target's onboarding, or regenerated by replaying the target's blocks from
  its creation.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = the beneficiary account, §5.2); the
                      Parachain Service queues it in `incoming_transfers`.
Phase 2: Book         pallet-jamkb moves the amount from `released`
                      account:
                      Valid memo: into the `custodial` account, recorded for
                      the beneficiary's Asset Hub account (`Custodial`).
                      Missing or malformed memo: into the `excess` account,
                      recorded with its source (`Excess`).
Phase 3: Claim        `claim(beneficiary)` moves the units from the
                      `custodial` account to the beneficiary.
```

---

## 5. Message Protocol

### 5.1 Operations, Correlation

An `OperationId` is unique and never reused. Every operation is resolved by
`settle`. A retry after a failure requires a new operation.

A `Delivery`, `Increase`, `Redemption`, `Reclaim` or `Refund` correlates by
id: its `TransferOut` carries the `OperationId` as its `id` field, and a
`TransferFailed { id }` entry points directly at the failed operation.

A recovery operation has no id in its message. Its failure entry
(`ServiceStoreFailed`, `ServiceEjectFailed`, `ServiceSupervisorFailed`)
names the service, so it correlates by target and entry
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

---

## 7. Open Issues

1. **Launch distribution.** At launch the existing parachains receive the JAMKB
   they need, system chains as a grant. The distribution contract comes later.
   How a grant to non-system chains relates to the referendum, which allows
   only selling JAMKB for DOT, is open.
2. **Storage footprint cleanup.** The pallet does not know the value length of
   a deleted key, so it cannot reclaim what a cleanup page freed. The freeing
   and the reclaim must also be atomic, since concurrent reclaims race for the
   freed funds and the caller of the cleanup should be the one to get them
   back.
3. **Storage key removal.** `ServiceStoreError` has no variant for a key that
   is not present, so `cleanup_storage` cannot tell whether keys were removed.
   `settle` slashes the deposit on Failed.
4. **Service Freeze.** No Parachain Service message stops a service from taking more
   state footprint, so a target can allocate storage while a cleanup removes
   it.
5. **Eject sweep.** `EjectService` credits the ejected service's balances to
   the Parachain Service without reporting the amount, so Asset Hub is not
   aware of the incoming transfer. Needs the Parachain Service to report the
   swept balance.
6. **Service accounts.** A mapping from JAM service to Asset Hub account is
   needed by the lease, for lease acceptance and early return, probably provided by
   another pallet.
7. **Generic transport.** The parachain-system rework that pulls the jamkb pallet's
   upward messages and delivers the validation inputs does not exist yet
   (§3.4).
8. **Pallet account and burn instead of holds.** A hold cannot take the last
   `min_balance` units and keeps the account alive, so a holder cannot redeem
   their full balance and close the account. One option is to move the units
   for permanent releases and redemptions into a pallet `in-flight` account
   instead, burn them on Confirmed, and return them on Failed. Leases would
   still use holds, since those units must return to the approver.
9. **Sufficiency.** Whether JAMKB is a sufficient asset, and its
   `min_balance`, is undecided. A sufficient asset needs a `min_balance`
   worth about the native existential deposit, which requires a price. If
   JAMKB is non-sufficient and `min_balance` is 1, the pallet accounts must
   be touched at `initialize`.
10. **Service lifecycle in another pallet.** Service accounts, service creation
    and upgrade, supervisor hand-over, forget and eject concern the service,
    not the token, and belong with the pallet that registers JAM services.
    They should probably leave this document.
