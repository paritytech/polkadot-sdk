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

At JAM genesis, the Parachain Service is expected to hold the full JAMKB
supply. Asset Hub manages distribution on behalf of the DAO.

Once every operation has settled, this holds:

```
PS regular balance = JAMKB total supply − released − leased
```

`released` is the units transferred to other JAM services; `leased` is the
units of delivered leases.

It can drift in two cases:

- The Parachain Service does not record an incoming transfer: it cannot pay
  for its queue entry, or its processing fails. The funds stay on the
  Parachain Service and Asset Hub does not see them.
- A failure entry is lost before Asset Hub reads it, because `parachain_log`
  was overwritten. Asset Hub settles a transfer that never happened.

Both can be addressed.

A third case cannot be addressed: a deferred transfer is discarded because
its destination was ejected before it was accumulated. The units are burnt on
JAM. Asset Hub still counts them as released.

Example flow for governance permanent release, from approval through settlement.

```
━━ Asset Hub block B: execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_release(target, amount, memo): the units are held on custody
   ▼
Any signed account
   │  execute_release(id)
   ▼
pallet-jamkb
   │  records the operation and adds a TransferOut to the send queue of
   │  pallet-parachain-system
   ▼
pallet-parachain-system
   │  calls the Parachain Service's send_upward_message(TransferOut{..})

━━ B's work report: Accumulate, on-chain on JAM ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Parachain Service
   │  transfer its balance → target's regular balance
   │  on failure it writes TransferFailed { id, error } to Asset Hub's
   │  parachain_log
   │  records B's header as Asset Hub's para head

━━ Asset Hub block C: a later block, its lookup-anchor at or past B's accumulation ━

pallet-parachain-system
   │  reads the para head and parachain_log; the head is B or later, so B
   │  is accumulated; delivers the operation's outcome with log entries
   ▼
pallet-jamkb
   │  records the outcome against the operation; then settles it:
   │  - Failed → the allocation returns to Approved
   │  - Confirmed → the units move to the `released` account
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

The full JAMKB cap is minted into the `pallet-jamkb` custody account (§3.2).
The mint is a one-time executed runtime call on the Asset Hub. `initialize`
takes a provider reference on each pallet account (§3.2). Governance is
required to set the metadata (symbol `JAMKB`, decimals 3) with
`force_set_metadata`.

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
| `unclaimed` | `into_sub_account_truncating(b"uncl")` | returns awaiting `claim` (§4.4) |
| `excess`    | `into_sub_account_truncating(b"xces")` | unattributed inflows (§4.4)     |

The Parachain Service regular balance is the one these accounts mirror.

Before any action like a permanent release or a lease is triggered, the units
are locked in place: a hold is placed on them, and only then does the transfer
execute on the JAM side. Every cross-system operation is asynchronous and is
finalized by settlement in `on_poll`. On confirmation the units of a lease stay held; the
units of a permanent release move into the `released` account. On failure the
pallet undoes what the call did, unless the payload says otherwise.

To resolve operations, the pallet receives each operation's outcome from the
parachain-system pallet (§3.4): the block that the message was sent in and
the `parachain_log` entries. A failure entry marks it `Failed`, otherwise it
is `Confirmed` (§5.1). The operations are settled in `on_poll` within its
weight budget. A failed settlement is retried by `settle` only.

A `Delivery`, `Increase` or `Reclaim` is an allocation operation; a `Freeze`,
`Unfreeze`, `Cleanup`, `Forget`, `Eject` or `Unsupervise` is a recovery
operation. The pallet keeps at most one unsettled operation per allocation
and at most one unsettled recovery operation per target; a call that would
record a second is rejected with `UnsettledOperation`.

For voluntary return (§4.4) it receives each `incoming_transfers` bucket
from parachain-system and books it. A valid beneficiary memo
credits `Unclaimed`; a missing or malformed memo credits `Excess`.

A transfer may land on the Parachain Service's supervisor balance. The pallet
books it and queues a plain transfer moving the amount from the supervisor
balance to the regular balance. It is queued before any operation that could
use the units.

The pallet starts inactive. Governance verifies, against public JAM state at a
defined block, that the Parachain Service balance and the balances of all
other services equal `CAP`. It records the result with
`attest(anchor, released)`, where `released` is the sum held by the other
services, 0 when everything is on the Parachain Service; the pallet stores the
anchor and moves `released` from custody to the `released` account. Until
attested, every call except `initialize` and `attest` is rejected.

```rust
/// The header hash of the JAM block whose state the balances were checked
/// against. The pallet records it and does not verify it.
type Anchor = Hash;

/// The fixed JAMKB supply in planks, minted at `initialize` (§3.1).
const CAP: Balance;

/// The minimum `amount` that can be transferred by
/// `approve_release`, `redeem`, `offer_lease`, `increase_lease`, `reclaim`,
/// and `dispose_excess` with `refund`. A smaller `amount` is rejected,
/// except a `reclaim` of the allocation's `amount`.
const MIN_TRANSFER: Balance;

/// The gas every deferred `TransferOut` carries (§5.2). A destination whose
/// `min_memo_gas` is above it refuses with `GasBelowDestinationMinimum`, and
/// the operation fails.
const MEMO_GAS: u64;

/// The weight `on_poll` spends per block settling operations (§3.2). At
/// least an `Eject` with `MAX_LEASES_PER_TARGET` leases.
const SETTLEMENT_WEIGHT: Weight;

/// A code hash with no preimage; a target set to it cannot run (§4.3).
const FROZEN_CODE_HASH: Hash = [0u8; 32];

type AllocationId = u64;              // assigned from 1;
type OperationId = u64;

/// A JAM service index.
type ServiceId = u32;

/// A JAM timeslot.
type Timeslot = u32;

/// Added to the lease end (`reclaim`, §4.3).
const GRACE_PERIOD: BlockNumber;

/// The cap on a target's accepted leases: those in `Delivering`, `Delivered`
/// or `Reclaiming`.
const MAX_LEASES_PER_TARGET: u32;

/// The recovery deposit (§4.3): a hold on the caller's native balance per
/// `cleanup_storage` page.
const RECOVERY_DEPOSIT: Balance;
/// `RECOVERY_DEPOSIT_PER_KEY` per key of a `cleanup_storage` page.
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

/// Awaiting `claim`, per beneficiary (§4.4).
type Unclaimed = Map<AccountId, Balance>;

/// Unattributed inflows, per source service (§4.4).
type Excess = Map<ServiceId, Balance>;

/// The frozen lease targets and the code hash, `min_acc_gas` and
/// `min_memo_gas` each had before its freeze (§4.3).
type FrozenTargets = Map<ServiceId, (Hash, u64, u64)>;

/// One approved allocation.
struct Allocation {
    id: AllocationId,
    mode: AllocationMode,             // Lease { duration } | Permanent { memo }
    target: ServiceId,
    amount: Balance,
    state: AllocationState,
    delivered_at: Option<BlockNumber>,
                                      // the block the delivery was sent in.
                                      // A lease ends at
                                      // `delivered_at + duration`
    approver: Option<AccountId>,      // the signing account or contract;
                                      // `None`: governance is the approver
    valid_from: BlockNumber,          // `accept_lease` and `execute_release`
                                      // are rejected before this block
    expires_at: BlockNumber,          // `accept_lease` and `execute_release`
                                      // are rejected from this block on
}

enum AllocationMode {
    Lease { duration: BlockNumber },
    Permanent { memo: [u8; 128] },    // the memo of the release (§5.2)
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
                                      // unset until reported (§3.4)
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
    /// set to the operation's `submitted_at`; for a release, the held units
    /// move from custody to the `released` account.
    Delivery {
        allocation: AllocationId,
        amount: Balance,              // the units locked for the transfer
    },
    /// A lease increase (`increase_lease`).
    /// One plain `TransferOut` from the Parachain Service's regular balance to
    /// the target's supervisor balance.
    ///
    /// Confirmed: the allocation's amount rises by `additional`; the lease
    /// stays `Delivered`.
    Increase {
        allocation: AllocationId,
        additional: Balance,
    },
    /// A redemption of holder funds (§4.2.2).
    /// One deferred `TransferOut` from the Parachain Service's regular balance
    /// to `target`'s regular balance, with the memo and gas of §5.2.
    ///
    /// Confirmed: the held units move from `holder` to the `released` account.
    Redemption {
        holder: AccountId,            // the redeemer; the hold sits on it
        target: ServiceId,
        amount: Balance,              // the units locked for the transfer
        memo: [u8; 128],              // the redeemer's memo (§5.2)
    },
    /// A lease return (`reclaim`, §4.3).
    /// One plain `TransferOut` from the lease target's supervisor balance to
    /// the Parachain Service's regular balance.
    ///
    /// Confirmed: `amount` is released from the hold and the allocation's
    /// amount falls by `amount`; a fully returned lease moves to `Closed`.
    /// Failed: a `Reclaim` sent past the lease end and `GRACE_PERIOD` moves a
    /// `Delivered` lease to `Reclaiming`; otherwise the lease keeps its state.
    Reclaim {
        allocation: AllocationId,
        amount: Balance,
    },
    /// A freeze (`freeze_target`, §4.3).
    /// One `UpgradeService { service: Some(target), code_hash:
    /// FROZEN_CODE_HASH, len: 0, min_acc_gas: 0, min_memo_gas: 0 }`.
    ///
    /// Confirmed, once `ServiceUpgraded { previous_code_hash,
    /// previous_min_acc_gas, previous_min_memo_gas }` is recorded: the three
    /// values are saved in `FrozenTargets`.
    Freeze {
        target: ServiceId,
    },
    /// An unfreeze (`unfreeze_target`, §4.3).
    /// One `UpgradeService { service: Some(target), code_hash, len: 0,
    /// min_acc_gas, min_memo_gas }` with the values saved in `FrozenTargets`.
    ///
    /// Confirmed: the target is removed from `FrozenTargets`.
    Unfreeze {
        target: ServiceId,
    },
    /// A cleanup page (`cleanup_storage`, §4.3).
    /// One `RemoveServiceStorage { service: target, key }` per key in `keys`.
    ///
    /// Confirmed: the deposit is released.
    /// Failed: the keys that were present are removed regardless;
    /// `RECOVERY_DEPOSIT_PER_KEY` is slashed to the treasury per missing key
    /// and the rest of
    /// the deposit is released.
    Cleanup {
        target: ServiceId,
        keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>,
        deposit: (AccountId, Balance),
    },
    /// A preimage release (`forget_preimage`, §4.3).
    /// One `Forget { target: Service(target), hash, len }`.
    ///
    /// Confirmed and Failed: no further state change. `ForgetAgainAt { due }`
    /// gives the timeslot of the second call.
    Forget {
        target: ServiceId,
        hash: Hash,
        len: u32,
    },
    /// An ejection (`eject_target`, §4.3).
    /// One `EjectService { service: target }`. It fails `NotEmpty` until the
    /// target holds no storage items and no preimage requests.
    ///
    /// Confirmed, once `ServiceEjected { credited }` is recorded: every
    /// accepted lease on the target moves to `Closed` and its hold is
    /// released; the rest of `credited` moves from the `released` account to
    /// the `excess` account, recorded as `Excess[target]` (§4.4). The target's
    /// `FrozenTargets` and `ServiceAccounts` entries are removed.
    /// Failed: the leases stay `Reclaiming`.
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
    /// A move of the Parachain Service's supervisor balance to its regular
    /// balance (§3.2). One plain `TransferOut` between the two. It cannot
    /// fail while the Parachain Service is its own supervisor.
    ///
    /// Confirmed: no further state change.
    Move {
        amount: Balance,
    },
    /// A refund of an incoming transfer to its source (`dispose_excess`).
    /// One deferred `TransferOut` from the Parachain Service's regular balance
    /// to `source`'s regular balance, with the memo and gas of §5.2.
    ///
    /// Confirmed: the held units move from the `excess` account to the
    /// `released` account.
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
    /// `ServiceUpgraded { service, previous_code_hash, previous_min_acc_gas,
    /// previous_min_memo_gas }` (§7.3); Freeze.
    ServiceUpgraded { previous_code_hash: Hash,
                      previous_min_acc_gas: u64,
                      previous_min_memo_gas: u64 },
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
    /// `ServiceUpgradeFailed { service, error }` (§7.3).
    ServiceUpgradeFailed(ServiceUpgradeError),
}

/// The booking class of an uncredited return (§4.4).
enum ReturnClass {
    Unclaimed,          // owner known, reserved for its claimant
    Excess,             // owner unknown, disposed by governance
}

/// Errors.
enum Error {
    NotAttested, Paused, BelowMinTransfer, WrongMode, WrongState, NotYetValid,
    Expired, NoServiceAccount, TargetIsParachainService, TooManyLeases,
    UnsettledOperation, NotFrozen, AlreadyFrozen, EjectUnsettled, NotResolved,
    CannotCreate, Transport,
}
```

Typed events:

```rust
enum Event {
    /// The genesis attestation is recorded.
    Attested { anchor: Anchor, released: Balance },
    ReleaseApproved { id: AllocationId, target: ServiceId, amount: Balance },
    LeaseOffered { id: AllocationId, target: ServiceId, amount: Balance,
                   duration: BlockNumber },
    AllocationCancelled { id: AllocationId },
    LeaseClosed { id: AllocationId, amount: Balance },
    LeaseIncreased { id: AllocationId, additional: Balance },
    LeaseExtended { id: AllocationId, duration: BlockNumber },
    /// `allocation` is `None` for a redemption, a refund, a move or a recovery
    /// operation.
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId, entry: Option<SuccessEntry> },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// The settlement of `id` failed with `error`; `settle` retries it
    /// (§3.2).
    SettlementFailed { id: OperationId, error: DispatchError },
    /// An amount was booked (§4.4): unclaimed for `beneficiary`, or excess
    /// when `beneficiary` is `None`, with the raw `memo` of the return.
    ReturnBooked { source: ServiceId, amount: Balance, class: ReturnClass,
                   beneficiary: Option<AccountId>, memo: [u8; 128] },
    /// A beneficiary's unclaimed balance was claimed.
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
/// custody and maps the custody account in pallet-revive.
fn initialize();

/// Records the genesis attestation against JAM state. Origin: governance
/// (Root). `released` is the JAMKB held by services other than the Parachain
/// Service at `anchor`; the released amount moves from custody to the
/// `released` account. Enables operations.
fn attest(anchor: Anchor, released: Balance);

/// Sets the pallet pause flag. Origin: governance (Root). While set, a call
/// that creates an allocation, records an operation or distributes units from
/// custody is rejected. Messages already queued are still processed.
fn pause();

/// Clears the pallet pause flag. Origin: governance (Root).
fn resume();

// ── Distribution from custody ──────────────────────────────────────────────

/// Grants a budget to a policy adapter contract (§1). Origin: governance
/// (Root). Transfers `amount` of undistributed units from the pallet's
/// custody to `beneficiary` on Asset Hub. A budget can return by an
/// ordinary transfer to `custody`.
fn grant(beneficiary: AccountId, amount: Balance);

// ── Permanent release ──────────────────────────────────────────────────────

/// Approves a permanent release to `target`. Origin: governance (Root).
/// Records `Allocation{mode: Permanent { memo }, state: Approved}` and holds
/// `amount` on custody (§3.3). `memo` goes to `target` unchanged (§5.2).
/// `valid_from` `None` means the current block. Rejected when `target` is the
/// Parachain Service.
fn approve_release(target: ServiceId, amount: Balance, memo: [u8; 128],
                   valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Executes an approved release. Origin: any signed account. Legal while
/// the allocation is `Approved`. Moves the allocation to `Delivering` and
/// records a `Delivery` operation.
fn execute_release(id: AllocationId);

/// Moves the caller's own units to `target`. Origin: any signed account. Holds
/// `amount` on the caller and records a `Redemption` operation with the
/// caller as `holder`. `memo` goes to `target` unchanged (§5.2). Rejected
/// when `target` is the Parachain Service.
fn redeem(amount: Balance, target: ServiceId, memo: [u8; 128]);

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
/// allocation's `amount`.
fn reclaim(id: AllocationId, amount: Balance);

/// Ends a lease that was not fully returned. Origin: the lease's approver or
/// governance. Legal while the lease is `Reclaiming`. Rejected while the
/// lease has an unsettled allocation operation (§3.2). The held units move
/// to the `released` account and the lease moves to `Closed`.
fn close_lease(id: AllocationId);

// ── Recovery: a lease target that does not cooperate ───────────────────────

/// Stops the target from taking more state footprint: its code hash is set
/// to `FROZEN_CODE_HASH`, so it no longer runs. Its code preimage stays in
/// place for `unfreeze_target`. Origin: any signed account. Legal while a
/// lease on the target is `Reclaiming` and the target is not recorded
/// frozen. Records a `Freeze` operation.
fn freeze_target(target: ServiceId);

/// Restores the code hash saved at the freeze. Origin: any signed
/// account. Legal while the target is recorded frozen and no lease on it is
/// `Reclaiming`. Records an `Unfreeze` operation.
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

/// Destroys the emptied target, crediting its regular and supervisor
/// balances to the Parachain Service's regular balance. Origin: the target's
/// service account or governance. Legal while
/// a lease on the target is `Reclaiming`. Rejected while a lease on the
/// target has an unsettled allocation operation, or a transfer operation to
/// the target is unsettled (§3.2). Records an `Eject` operation.
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
/// `unclaimed` account to the `beneficiary` account. Rejected with
/// `CannotCreate` when `beneficiary` has no JAMKB account and no native
/// balance to create one; the unclaimed balance stays booked.
fn claim(beneficiary: AccountId);

/// Disposes an excess amount per service. Origin: governance (Root). Lowers
/// `Excess[source]` by `amount`. With `refund = true` it holds `amount` on the
/// `excess` account and records a `Refund` operation sending the units back to
/// the `source` service's regular balance, with `memo` unchanged (§5.2); with
/// `refund = false` it moves the units from the `excess` account to custody
/// and `memo` is ignored.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool,
                  memo: [u8; 128]);

// ── Shared by every flow ───────────────────────────────────────────────────

/// Cancels an allocation in `Approved`. Origin: the allocation's approver, or
/// any signed account from `expires_at` on. The hold is released and
/// the allocation moves to `Closed`. A `Delivering` allocation cannot be
/// cancelled.
fn cancel_allocation(id: AllocationId);

/// Retries a failed settlement (§3.2). Origin: any signed account.
/// Rejected unless the operation's settlement has failed.
fn settle(op_id: OperationId);
```

#### 3.2.1 pallet-jamkb precompile

Contracts access the pallet through a precompile at a fixed address. It
exposes the calls and reads a contract needs. Each precompile function calls
the internal pallet method that holds the logic shared with the dispatchable
of the same name. The call origin is the account that calls the precompile.
Every account in the interface is an AccountId32 passed as `bytes32`.
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
    enum AllocationState { Approved, Delivering, Delivered, Reclaiming, Closed }
    enum AllocationMode { Lease, Permanent }
    enum OperationState { NotFound, Requested, Confirmed, Failed }

    struct Allocation {
        uint64 id;
        AllocationState state;
        AllocationMode mode;
        uint32 target;
        uint128 amount;
        bytes32 approver;      // bytes32(0) for governance
        uint32 deliveredAt;    // 0: not delivered
        uint32 duration;       // 0 for Permanent
        uint32 validFrom;
        uint32 expiresAt;
    }

    // -- Permanent release --
    function executeRelease(uint64 allocationId) external returns (uint64 opId);
    function redeem(uint128 amount, uint32 target, bytes32[4] calldata memo)
        external returns (uint64 opId);

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
    function claim(bytes32 beneficiary) external returns (uint128 claimed);

    // -- Shared --
    function cancelAllocation(uint64 allocationId) external;

    // -- Reads. allocation() reverts for an unknown id. --
    function allocation(uint64 allocationId) external view returns (Allocation memory);
    /// `NotFound` for an id the pallet does not hold: settled, or never issued.
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
    function maxLeasesPerTarget() external view returns (uint32);
    function recoveryDeposit() external view returns (uint128);
    function recoveryDepositPerKey() external view returns (uint128);
    function maxKeysPerPage() external view returns (uint32);
}
```

### 3.3 pallet-assets access

`pallet-assets` holds JAMKB. `pallet-jamkb` holds the four roles and
administers the asset through the runtime-internal fungibles traits:
`transfer`, `hold`, `release`, `transfer_on_hold` and `mint_into`. Policy adapters
move their JAMKB balances through the ERC20 precompile.

**Hold rules.** `pallet-jamkb` locks units with a hold before it sends a
transfer. The runtime sets `pallet-assets-holder` as the asset's `Holder`; it
provides the hold traits. A hold merges per reason per account, so the amount
of each lease and each recovery deposit is kept in pallet storage. The hold
reasons are declared as:

```rust
/// The hold classes.
enum HoldReason {
    /// Units backing a lease.
    #[codec(index = 0)]
    Leased,
    /// Units backing outstanding permanent releases.
    #[codec(index = 1)]
    Releasing,
    /// The native-currency recovery deposit (`RECOVERY_DEPOSIT`).
    #[codec(index = 2)]
    RecoveryDeposit,
}
```

`Leased` and `Releasing` hold JAMKB through `pallet-assets-holder`;
`RecoveryDeposit` holds the native currency through `pallet-balances`.

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

It holds the send queue, as `parachain-system` does for upward XCM today:
`pallet-jamkb` adds each operation's messages to it, and it sends them
through `send_upward_message` within the limits below.

The transport layer responsibilities:

- the outcome of each operation: the block that sent it and its
  `parachain_log` entries, reported once, in send order, in the block whose
  anchor first covers it;
- `incoming_transfers` delivered as whole buckets, up to the number
  `pallet-jamkb` can process; a bucket not processed is delivered again;
- `CleanUpBucketsUpTo` sent after the processed `incoming_transfers` buckets;
- the Parachain Service balances delivered for drift detection;
- a send queue within 1024 messages and 40 KiB per block, FIFO, with a
  sharing policy between pallets;
- gas accounting for the send queue messages: a limit on how much gas the
  messages of one block cost, shared between pallets. Each deferred
  `TransferOut` costs JAM's fixed transfer cost, `C_gasT` (Gray Paper), plus
  the memo gas. The limit is large enough
  for any single message, so every queued message is sent; messages that do
  not fit in this block are sent in a later one;
- a bound on the log entries not yet delivered: each message counts the
  largest entry it can cause in `parachain_log` until the entries of its
  block are delivered, and the messages of one operation are sent in one
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
Phase 1: Offer        `offer_lease(target, amount, duration, ..)` records the
                      lease offer.
Phase 2: Accept       `accept_lease(id)` starts delivery to the target's
                      supervisor balance.
Phase 3: Confirm       The operation settles, the output:
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
Phase 1: Approve      `approve_release(target, amount, memo, ..)` records the
                      release allocation.
Phase 2: Execute      `execute_release(id)` starts delivery to the target's
                      regular balance.
Phase 3: Confirm       The operation settles, the output:
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
Phase 1: Redeem       `redeem(amount, target, memo)` starts delivery to
                      the target's regular balance.
Phase 2: Confirm       The operation settles, the output:
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
Phase 3: Confirm       The operation settles, the output:
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
For a lease from a DAO adapter, governance may fund the recovery as a treasury
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
Phase 5: Confirm       The eject settles; the holds of the target's leases are
                      released; the leases move to Closed.
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
Phase 2: Book         pallet-jamkb moves the amount from `released`
                      account:
                      Valid memo: into the `unclaimed` account, recorded for
                      the beneficiary's Asset Hub account (`Unclaimed`).
                      Missing or malformed memo: into the `excess` account,
                      recorded with its source (`Excess`).
Phase 3: Claim        `claim(beneficiary)` moves the units from the
                      `unclaimed` account to the beneficiary.
```

---

## 5. Message Protocol

### 5.1 Operations, Correlation

An `OperationId` is unique and never reused.
An operation is resolved when parachain-system reports its outcome and its log
entries (§7.8).

A `Delivery`, `Increase`, `Redemption`, `Reclaim`, `Refund` or `Move` sends one
`TransferOut` carrying a `MessageId`. A `TransferFailed`
entry in its outcome means it failed.

A recovery operation's messages carry no id. Their entries
(`ServiceKeyNotFound`, `ServiceForgetNotRequested`, `ServiceEjectFailed`,
`ServiceSupervisorFailed`, `ServiceUpgradeFailed`, `ServiceEjected`,
`ServiceUpgraded`, `ForgetAgainAt`) name the service, and parachain-system attributes them to the
one operation of that kind in flight on that service (§3.2).

### 5.2 Memo Requirements

JAM transfer memos are 128 octets. A redemption, a release and a refund
carry the memo their caller supplies, in a format the destination service
defines. Every deferred `TransferOut` carries `MEMO_GAS`. A service that
receives JAMKB from Asset Hub keeps its `min_memo_gas` at or below
`MEMO_GAS`; a transfer to a service above it is refused and undone. A
voluntary return carries the beneficiary account as its first 32 bytes, the
rest zero.

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
   and the reclaim must also be atomic: with several leases on one target,
   their reclaims race for the freed funds, and the lease the cleanup was done
   for should be the one to get them back.
3. **Service Freeze.** The Parachain Service does not log anything for a supervised
   `UpgradeService`. The freeze needs `ServiceUpgraded { service,
   previous_code_hash, previous_min_acc_gas, previous_min_memo_gas }` so that
   `unfreeze_target` can restore them, and `ServiceUpgradeFailed { service,
   error }` so that a refused upgrade is not `Confirmed`.
4. **Burn instead of holds.** A hold cannot take the last plank, so a holder
   redeems all but one plank and transfers the remainder out by a plain
   `pallet-assets` transfer. The alternative is `fungibles::Mutate::shelve`
   on the call and `restore` on Failed; `restore` into a dead non-sufficient
   account fails, so those units would fall back to `Unclaimed`.
5. **Service registry pallet.** Service accounts, supervision, service
   creation and upgrade belong to a service registry pallet that does not
   exist. The recovery calls of §4.3 stay here: they apply their origin rule
   and the recovery deposit, then request through `ServiceRegistry`. The
   registry is the only sender of service-management messages. It keeps one
   pending request per service, takes its outcome from parachain-system, and
   reports it through `OnServiceEvent`. Before its own
   dispatchables send it asks
   `ServiceGuard`. A `CreateService` is paid by its creator in JAMKB through
   `CreationTicket`, which the pallet implements and the registry holds
   until the outcome.

   ```rust
   /// Implemented by the service registry. A request is sent in the call
   /// through parachain-system; it takes effect in a later block and is
   /// reported through `OnServiceEvent`.
   trait ServiceRegistry {
       /// A raw storage key of a service, and the keys in one page.
       type Key;
       type MaxKeysPerPage: Get<u32>;
       /// The Asset Hub account of `service`, if registered.
       fn account(service: ServiceId) -> Option<AccountId>;
       /// Whether the Parachain Service supervises `service`.
       fn is_supervised(service: ServiceId) -> bool;
       /// Whether `service` is frozen.
       fn is_frozen(service: ServiceId) -> bool;
       /// Freezes `service`: it stops executing.
       fn freeze(service: ServiceId) -> Result<(), ServiceError>;
       /// Unfreezes `service`: its stored code is restored.
       fn unfreeze(service: ServiceId) -> Result<(), ServiceError>;
       /// Removes `keys` from `service`'s storage.
       fn remove_storage(service: ServiceId,
                         keys: BoundedVec<Self::Key, Self::MaxKeysPerPage>)
           -> Result<(), ServiceError>;
       /// Forgets the preimage `(hash, len)` of `service`.
       fn forget(service: ServiceId, hash: Hash, len: u32)
           -> Result<(), ServiceError>;
       /// Ejects `service`.
       fn eject(service: ServiceId) -> Result<(), ServiceError>;
       /// Sets `service`'s supervisor.
       fn set_supervisor(service: ServiceId, new_supervisor: ServiceId)
           -> Result<(), ServiceError>;
   }

   /// A synchronous rejection. Nothing is sent.
   enum ServiceError {
       NotRegistered, NotSupervised, Frozen, AlreadyFrozen, NotFrozen,
       NoCode,
   }

   /// Implemented by `pallet-jamkb`. Read by the registry before it sends.
   trait ServiceGuard {
       /// Whether `service`'s supervisor may be set, to itself or another.
       fn may_set_supervisor(service: ServiceId) -> bool;
       /// Whether `service` may be unfrozen.
       fn may_unfreeze(service: ServiceId) -> bool;
       /// Whether `service` may be ejected.
       fn may_eject(service: ServiceId) -> bool;
   }

   /// Implemented by `pallet-jamkb`: the JAMKB cost of a `CreateService`.
   /// `new` holds the new service's threshold balance on the creator; `burn`
   /// moves it to the `released` account on `Created`; `drop` releases it
   /// otherwise. The registry keeps it by `CreateService.id`.
   type CreationTicket: Consideration<AccountId, Footprint>;

   /// The outcome of a `Forget`.
   enum ForgetOutcome {
       /// The preimage is expunged.
       Expunged,
       /// The preimage is unrequested; a second `Forget` from the carried
       /// timeslot expunges it.
       AgainAt(Timeslot),
       /// The service never requested the preimage.
       NotRequested,
   }

   /// Implemented by `pallet-jamkb`. Called by the registry once per
   /// request, after the registry's settlement. It cannot fail and charges what it writes
   /// to `meter`.
   trait OnServiceEvent {
       /// `service` was created and its `CreationTicket` burned. Its threshold
       /// balance is on its supervisor balance when `new_supervisor_balance`
       /// is set, on its regular balance otherwise.
       fn on_created(service: ServiceId, new_supervisor_balance: bool,
                     meter: &mut WeightMeter);
       /// A freeze or unfreeze of `service` was executed.
       fn on_frozen(service: ServiceId, frozen: bool,
                    meter: &mut WeightMeter);
       /// A storage page of `service` was executed. `missing` holds each key
       /// the service did not hold.
       fn on_storage_removed(service: ServiceId,
                             missing: BoundedVec<Hash, MAX_KEYS_PER_PAGE>,
                             meter: &mut WeightMeter);
       /// A `Forget` of `service` was executed.
       fn on_forgotten(service: ServiceId, outcome: ForgetOutcome,
                       meter: &mut WeightMeter);
       /// An `EjectService` of `service` was executed. `Ok` carries
       /// `credited`.
       fn on_ejected(service: ServiceId,
                     outcome: Result<Balance, ServiceEjectError>,
                     meter: &mut WeightMeter);
       /// A `SetServiceSupervisor` of `service` was executed.
       fn on_supervisor_set(service: ServiceId,
                            outcome: Result<(), ServiceSupervisorError>,
                            meter: &mut WeightMeter);
   }
   ```

   The jamkb pallet's `OperationPayload` variants record a pending request,
   keyed by service. The hook only records the outcome against it.
6. **Forced head.** Coretime's `ParachainSetHead` (Parachain Service design
   §6.3) moves Asset Hub's head, and its state, without accumulating
   anything. JAM's state does not follow, so the accounting diverges.
7. **Parachain Service supervisor.** A `Move` needs the Parachain Service to
   be its own supervisor. The Parachain Service design has to guarantee it.
8. **parachain-system interface.** parachain-system interface proposal is as follows:

   ```rust
   /// Each sender has its own budget share.
   type SenderId = u8;
   /// The id a `TransferOut` or `CreateService` carries, echoed by the
   /// Parachain Service in `TransferFailed` and `ServiceCreation`.
   type MessageId = u64;
   /// One atomic batch of upward messages.
   type OperationId = u64;

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
       /// in send order. `sent_at` is the block that carried its messages;
       /// `entries` are its `parachain_log` entries, at most one per
       /// message. A sender gets only the outcomes of its own operations.
       /// It cannot fail and charges what it writes to `meter`.
       fn on_outcome(
           sender: SenderId,
           id: OperationId,
           sent_at: BlockNumber,
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
       /// `capacity()` buckets per block. The pallet books the bucket in
       /// the call, all or nothing, and charges it to `meter`. `Ok`: the
       /// bucket is cleaned up and not delivered again. `Err`: the
       /// bucket was not booked. Parachain-system rolls the call back,
       /// delivers no more buckets this block, cleans up only the ones
       /// before it, and delivers it again in a later block.
       fn on_incoming_transfers(
           transfers: IncomingTransfers,
           meter: &mut WeightMeter,
       ) -> Result<(), ()>;
   }
   ```
