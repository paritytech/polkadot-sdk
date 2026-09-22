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
5. [Cap & Backing Accounting](#5-cap--backing-accounting)
6. [Message Protocol](#6-message-protocol)
   - 6.1 [Operations, Correlation](#61-operations-correlation)
   - 6.2 [Memo Requirements](#62-memo-requirements)
7. [References](#7-references)

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

The detailed flow below is a governance-executed permanent release (§4.2): one
deferred transfer from the Parachain Service balance to the target's regular
balance.

```
━━ Asset Hub block B — execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_release(target, amount)
   │  execute_release(id)
   ▼
pallet-jamkb
   │  holds the units (§3.1), records the operation, and appends a
   │  TransferOut to PendingOperations
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
   │    operation's id → Confirmed
   │  - a TransferFailed for the operation's id → Failed, the hold is released
   │  - para head not yet at B → the operation stays pending
```

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. This asset is managed by `pallet-jamkb`. It
holds the four privileged roles (Owner, Issuer, Admin, Freezer), assigned to it
at initialization. The pallet account has no key, so no external account can
administer the asset.

The full JAMKB cap is minted into the pallet's account, so that Asset Hub
holds a 1:1 representation of the JAM-side balance. The mint is a one-time
governance-executed runtime call on the Asset Hub.

At bootstrap some JAM services need a balance to operate. The DAO supplies them
at `initialize`, and the amount is held under `Released` (§5).

Parachain state footprint is backed on the Coretime chain. JAMKB is teleported
there, and the Coretime chain backs each parachain's footprint against what it
holds. Teleported units leave Asset Hub's spendable supply and park in the XCM
checking account (§5); the cap does not move.

### 3.2 `pallet-jamkb`

A FRAME pallet that manages the JAMKB asset and executes transfer operations
against the Parachain Service. Before any action like a permanent release or a
supervisor-managed allocation is triggered, the tokens are locked in place on
the owner account: a hold is placed on them, and only then does the transfer
execute on the JAM side. On confirmation the tokens stay held: in place for a
lease, in the pallet's custody for a release (moved there if they were not on
the pallet's account). On failure the hold is released.

The pallet also supports voluntary return (§4.4): a JAM service sends its
funds back to the Parachain Service balance, setting an Asset Hub beneficiary
in the transfer memo. The pallet records the returned amount as claimable, and
the beneficiary claims the units back to its Asset Hub account.

A JAM service has no Asset Hub account of its own, so the pallet keeps a
mapping of service accounts (`ServiceAccounts`): the Asset Hub account
associated with a JAM service. Its only power is to accept a lease offer for
that service. An approval is only an offer, and the target's service account
accepts it; a service with no registered account cannot be a lease target.

The recovery calls (§4.3) take a deposit on the caller's native balance, under
the `RecoveryDeposit` hold reason on `pallet-balances`: a runtime-constant base
per operation, and a per-key part for a `cleanup_storage` page. It is released
on a Confirmed settle and burned on a Failed one.

The pallet starts inactive. Governance verifies, against public JAM state at a
defined block, that the JAM balances match the pallet's configuration: the
Parachain Service holds `Cap − released`, and the other JAM services hold the
rest. It records the result with `attest(anchor)`; the pallet stores the anchor.
Until attested, every call except `initialize` and `attest` is rejected.

```rust
/// A JAM block reference. The attestation sets the block whose state the
/// balances were checked against (§3.1). The pallet records it and does not
/// verify it.
struct Anchor {
    timeslot: Timeslot,
    header_hash: Hash,
}

/// A raw storage key of a supervised service (required for `cleanup_storage`,
/// §4.3).
type Key = BoundedVec<u8, MAX_KEY_LEN>;

/// The Asset Hub account associated with a JAM service.
type ServiceAccounts = Map<ServiceId, AccountId>;

/// The frozen lease targets (§4.3).
type FrozenTargets = Set<ServiceId>;

/// The account a lease is assigned to (`assign_lease`).
type LeaseAssignments = Map<AllocationId, AccountId>;

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
                                      // `delivered_at + duration` (§4.3)
    approver: PalletsOrigin,          // who approved it: a signed origin
                                      // holding the units (an account or a
                                      // contract), or governance
    valid_from: BlockNumber,          // execute is rejected before this block
    expires_at: BlockNumber,          // execute is rejected from this block
                                      // on; only cancel_allocation still
                                      // works, and it releases the hold
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
    Reclaiming,                       // the cooperative return did not
                                      // complete; Lease only
    Closed,                           // cancelled or returned
}

/// One cross-system operation between Asset Hub and JAM.
struct Operation {
    id: OperationId,                  // unique operation Id (§6.1)
    payload: OperationPayload,
    state: OperationState,
    submitted_at: BlockNumber,
}

/// The operation's data payload.
enum OperationPayload {
    /// A lease or a governance permanent release.
    /// One `TransferOut` crediting the allocation's target (§4.1, §4.2.1);
    /// deferred when the allocation's mode is `Permanent`.
    Delivery {
        allocation: AllocationId,
        amount: Balance,              // the units locked for the transfer (§5)
    },
    /// A redemption of holder funds (§4.2.2).
    /// One deferred `TransferOut` crediting `target`'s regular balance from a
    /// holder's units.
    Redemption {
        target: ServiceId,
        amount: Balance,              // the units locked for the transfer (§5)
    },
    /// One plain `TransferOut` debiting the lease target's supervisor balance
    /// and crediting the Parachain Service (§4.3). A Confirmed settle reduces
    /// the allocation's amount and releases that much of the `Leased` hold
    /// (§3.3); the lease ends at zero. A Failed settle moves the lease to
    /// `Reclaiming`.
    Reclaim {
        allocation: AllocationId,
        amount: Balance,
    },
    /// A recovery of leased funds.
    /// One recovery call (§4.3): one message, except a `cleanup_storage` page,
    /// which sends one `RemoveServiceStorage` per key.
    Recovery {
        allocation: AllocationId,
        deposit: Option<(AccountId, Balance)>,
                                      // the caller and its deposit hold;
                                      // absent when no deposit was
                                      // placed (§4.3)
    },
    /// A refund of an incoming transfer back to its service.
    /// One deferred `TransferOut` returning an excess amount to its source
    /// (`dispose_excess`).
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
/// §3.3).
enum FailureReason {
    Transfer(TransferError),            // a TransferFailed entry (§6.1)
    Store(ServiceStoreError),           // a ServiceStoreFailed entry
    Eject(ServiceEjectError),           // a ServiceEjectFailed entry
    Supervisor(ServiceSupervisorError), // a ServiceSupervisorFailed entry
}

/// The booking class of an uncredited return (§4.4).
enum ReturnClass {
    Custodial,          // owner known, reserved for its claimant (§5)
    Excess,             // owner unknown, disposed by governance (§5)
}
```

Aggregates maintained for reporting and the conservation check (§5):

```rust
struct Totals {
    excess: Balance,           // unattributed inflows: donations, bad-memo
                               // returns; kept per source service; owner
                               // unknown, disposed by governance (§5)
    custodial: Balance,        // available for claim: sourced returns (§4.4);
                               // owner known
    in_flight_out: Balance,    // outbound deferred transfers sent and not
                               // yet settled: releases and redemptions (§5)
    released: Balance,         // permanent releases to other services
    leased: Balance,           // active supervisor-balance allocations
}
```

Typed events:

```rust
enum Event {
    /// The genesis attestation is recorded (§3.1).
    Attested { anchor: Anchor },
    AllocationApproved { id: AllocationId, mode: AllocationMode,
                         target: ServiceId, amount: Balance },
    AllocationCancelled { id: AllocationId },
    /// `allocation` is `None` for a redemption or a refund.
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// An amount was booked (§4.3, §4.4): custodial for a named beneficiary,
    /// or excess when the owner is unknown (a bad-memo return).
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
/// Origin: governance (Root). Mints `Cap` into the pallet's custody and sets
/// `Released` for the JAM services endowed at genesis. Rejected unless
/// `released <= Cap`. The pallet stays inactive until attested.
fn initialize(released: Balance);

/// Origin: governance (Root). Records the genesis attestation against JAM
/// state; enables operations (§3.1).
fn attest(anchor: Anchor);

/// Origin: governance (Root). Records `Allocation{mode: Permanent, state:
/// Approved}` and holds `amount` under `Released` in the pallet's custody
/// (§3.3).
fn approve_release(target: ServiceId, amount: Balance,
                   valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Origin: a token holder or governance. Records `Allocation{mode: Lease,
/// state: Approved}` and holds `amount` on the approver's account (§3.3).
/// Rejected if the target is recorded frozen, has no registered service
/// account, or already has an accepted lease that is not `Closed`.
fn offer_lease(target: ServiceId, amount: Balance, duration: BlockNumber,
               valid_from: Option<BlockNumber>, valid_for: BlockNumber);

/// Origin: the allocation's approver. Cancels an allocation in `Approved`: the
/// hold is released and the allocation state moves to `Closed`. The call stays
/// legal past `expires_at`. A `Delivering` allocation cannot be cancelled.
fn cancel_allocation(id: AllocationId);

/// Origin: governance (Root). Sets and clears the pallet pause flag, checked
/// by every entry point except `settle`. Messages already queued are
/// still processed.
fn pause();
fn resume();

/// Origin: any signed account. Legal while the allocation is `Approved`, at
/// or after `valid_from` and before `expires_at`. Each execution creates a
/// new delivery operation; a failed one is terminal. Moves the
/// allocation to `Delivering` and records the delivery operation.
fn execute_release(id: AllocationId);

/// Origin: the target's service account (`ServiceAccounts`). Legal while the
/// allocation is `Approved`, at or after `valid_from` and before
/// `expires_at`. Each execution creates a new delivery operation; a failed
/// one is terminal. Moves the allocation to `Delivering` and records
/// the delivery operation.
fn accept_lease(id: AllocationId);

/// Origin: any signed account. Legal while the lease is `Reclaiming`. Stops
/// the target from taking more state footprint and records it in
/// `FrozenTargets`; a Failed settle clears the record. The Parachain Service
/// has no support for this.
fn freeze_target(target: ServiceId);

/// Origin: the lease's approver or governance while the lease is
/// `Reclaiming`; any signed account with the recovery deposit once it is
/// `Closed` and fully returned. Reverses the freeze; a Confirmed settle
/// clears the `FrozenTargets` record.
fn unfreeze_target(target: ServiceId);

/// Origin: any signed account, with the recovery deposit, refunded in
/// proportion to the keys the page removed. Legal while the lease is
/// `Reclaiming`. Deletes one bounded page of keys from the target's storage,
/// each sent as `RemoveServiceStorage { service, key }`.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Origin: any signed account, with the recovery deposit. Legal while the
/// lease is `Reclaiming`. Releases a solicited preimage: a `Forget` upward
/// message. Forgetting the target's code preimage is restricted: governance
/// at any time, the approver only after a notice period since the freeze.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Origin: any signed account, with the recovery deposit. Legal while the
/// lease is `Reclaiming`. Destroys the emptied target (`EjectService`),
/// crediting its balances to the Parachain Service; a Confirmed settle moves
/// the lease to `Closed` and releases the `Leased` hold. The swept amount
/// counts as the lease return up to `leased`, any surplus as excess (§5),
/// which needs the sweep enqueued as an `incoming_transfers` entry; the
/// Parachain Service does not provide it.
fn eject_target(target: ServiceId);

/// Origin: any signed account. Releases the supervised target to itself
/// (`SetServiceSupervisor`). Rejected while the target is recorded frozen,
/// or has a lease that is not `Closed` and fully returned.
fn unsupervise(target: ServiceId);

/// Origin: the lease's approver past `delivered_at + duration + grace`, or
/// the target's service account at any time. Legal from `Delivered` and from
/// `Reclaiming`. Creates `Operation{Reclaim}` for `amount`, capped at the
/// allocation's remaining amount.
fn reclaim(id: AllocationId, amount: Balance);

/// Origin: the lease's approver or governance. Legal while the lease is
/// `Reclaiming`. The `Leased` hold moves into the pallet's custody under
/// `Released` and the lease moves to `Closed`.
fn close_lease(id: AllocationId);

/// Origin: the lease's approver. Legal while the lease is `Delivered`. Raises
/// `amount` by `additional`, holds that much more under `Leased` on the
/// approver's account and records a delivery operation crediting the target's
/// supervisor balance. The lease end does not change.
fn increase_lease(id: AllocationId, additional: Balance);

/// Origin: the lease's approver. Legal while the lease is `Delivered`. Sets
/// the lease's `duration`. Sends no message.
fn extend_lease(id: AllocationId, duration: BlockNumber);

/// Origin: the lease's approver. Legal while the lease is `Delivered`.
/// Records `to` in `LeaseAssignments`; a later call replaces the entry.
fn assign_lease(id: AllocationId, to: AccountId);

/// Origin: the account recorded in `LeaseAssignments`. `amount` is held under
/// `Leased` on the caller, the previous approver's hold is released,
/// `approver` becomes the caller and the entry is cleared. Sends no message.
fn take_over_lease(id: AllocationId);

/// Origin: governance (Root). Transfers `amount` of undistributed units from
/// the pallet's custody to `beneficiary` on Asset Hub: a budget for a policy
/// adapter contract (§1).
fn grant(beneficiary: AccountId, amount: Balance);

/// Origin: any JAMKB holder. Moves the caller's own units to the destination
/// JAM service.
fn redeem(amount: Balance, dest: ServiceId);

/// Origin: any signed account. Claims the voluntarily returned balance to
/// `beneficiary`'s account (§4.4): moves the units from custody, releasing
/// the amount from the `Released` hold.
fn claim(beneficiary: AccountId);

/// Origin: governance (Root). Disposes an excess amount per service (§5).
/// With `refund = true` it creates a `Refund` operation sending the tokens
/// back to the `source` service's regular balance; with `refund = false` it
/// accounts the amount as undistributed custody.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool);

/// Origin: any signed account. Updates the operation state to Confirmed or
/// Failed. For an operation already marked `Failed` at delivery (§3.4), it
/// releases the hold and updates the allocation.
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
transfer. The runtime wires `pallet-assets-holder` as the asset's `Holder`;
it provides the hold traits. The hold reason is declared in the runtime as
one closed set. The hold classes:

```rust
/// The hold classes.
enum HoldReason {
    /// Units backing an active lease (§4.1).
    #[codec(index = 0)]
    Leased,
    /// Units backing outstanding permanent releases (§4.2).
    #[codec(index = 1)]
    Released,
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
`incoming_transfers`) to pallets as validation inputs.

#### What the pallet owns

`pallet-jamkb` appends each operation's messages to `PendingOperations`, the
per-block send queue. The parachain-system pallet takes the queue through the
source trait and calls `send_upward_message` for each message; the operation
moves to `Submitted`. The runtime `Config` sets `pallet-jamkb` as the only
provider of the message variants it sends. `pallet-jamkb` hands over at most a
runtime-constant number of operations per block; the rest stay queued. That cap
and `MAX_KEYS_PER_PAGE` are sized so that Asset Hub's worst-case block fits the
Parachain Service's accumulate gas allocation.

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
Precondition: the Parachain Service is the target's effective supervisor.

```
Phase 1: Offer        Any token holder or governance calls
                      `offer_lease(target, amount, duration, ..)`.
                      `amount` is held on the approver's account.
Phase 2: Accept       The target's service account (§3.2) calls
                      `accept_lease(id)`. `pallet-jamkb` moves the
                      allocation to Delivering and queues a TransferOut
                      crediting the target's supervisor balance (§3.4).
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the units stay held under `Leased` on the
                      approver's account, backing the lease (§5); the
                      allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the hold on the
                      approver is released (§5); the allocation is back to
                      Approved.
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
Phase 1: Approve      Governance calls `approve_release(target, amount, ..)`;
                      `amount` is held under `Released` in pallet custody
                      (§3.3).
Phase 2: Execute      Any signed account calls `execute_release(id)` (§3.2).
                      The pallet moves the allocation to Delivering and queues
                      a TransferOut crediting the target's regular balance
                      (§3.4).
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units stay held
                      under `Released`; the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the hold is released;
                      the allocation is back to Approved.
```

#### 4.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service,
bypassing governance:

```
Phase 1: Redeem       Holder calls `redeem(amount, dest)` (§3.2). The pallet
                      places a hold on the holder's units (§3.3) and queues a
                      TransferOut crediting the target's regular balance
                      (§3.4).
Phase 2: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 3: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance; the held units move into the pallet's custody,
                      accounted under `Released` (§3.3).
                      Failed: JAM rejected the transfer; the hold on the
                      holder's units is released.
```

### 4.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reclaim      The approver or the target's service account calls
                      `reclaim`; the pallet queues a TransferOut debiting the
                      target's supervisor balance by the requested amount
                      (§3.4). A partial amount is legal.
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4). It fails if, after the
                      debit, balance + supervisor_balance < the threshold
                      balance.
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: that much of the `Leased` hold on the
                      approver is released (§5); at zero the lease moves to
                      Closed and the pallet queues unsupervise(target).
                      Failed: JAM rejected the transfer; the lease moves to
                      Reclaiming.
```

Full return, non-cooperative. Entered when a reclaim has failed, leaving the
lease `Reclaiming`.

Supervision gives the pallet full power over the target, including cleaning
its state and ejecting it. Any signed account runs the recovery;
`cleanup_storage`, `forget_preimage` and `eject_target` are bonded with a
deposit (§3.2). For a governance-approved lease, governance may fund the work
as a treasury bounty:

```
Phase 1: Freeze       Anyone submits `freeze_target` (§3.2) while the lease is
                      Reclaiming.
Phase 2: Cleanup      Anyone submits `cleanup_storage` pages (§3.2), and
                      `forget_preimage` for every preimage except the
                      target's code preimage.
                      The exit fork:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; the approver or
                          governance calls `unfreeze_target` (§3.2);
                          unsupervise(target) ends the enforced cleanup,
                          leaving the target self-supervised.
                      (b) TERMINATE: continue below.
Phase 3: Forget       The approver or governance discards the code preimage:
                      `forget_preimage`; eject fails `NotEmpty` until the
                      preimage is expunged.
Phase 4: Eject        Anyone submits `eject_target` (§3.2).
Phase 5: Confirm      The eject settles by a `settle(op_id)` call on the
                      pallet (§6.1); the lease-return hold is released (§5);
                      the lease moves to Closed.
```

- Storage keys are not recoverable from JAM state. They are tracked from the
  target's onboarding, or regenerated by replaying the target's blocks from
  its creation, so a cleanup can always be completed.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = return attribution, §6.2); the Parachain
                      Service queues it in `incoming_transfers`.
Phase 2: Claim        The pallet processes the entry in the queue (§3.4). The
                      output:
                      Valid memo (§6.2): the amount is booked as custodial for
                      the named Asset Hub account (§5); the beneficiary
                      collects it with the permissionless `claim` (§3.2).
                      Missing or malformed memo: the amount is booked as
                      excess (§5), recorded with its source; a refund to the
                      source is a governance disposition (`dispose_excess`,
                      §3.2).
```

- The Parachain Service cannot refuse an incoming transfer. JAM credits the
  destination before its code runs. Its only decision is whether the transfer is
  recorded. The queue's reserved portion (`MAX_INCOMING_TRANSFERS`) records
  unconditionally. Beyond it the queue is self-funding: an entry is recorded
  only if the transferred `amount` covers its own queue-slot cost. Below that
  floor the funds are kept but the transfer goes unrecorded. Without a
  governance action they stay unusable.

---

## 5. Cap & Backing Accounting

Conservation:

```
cap  =  reserve + locked + teleported

reserve        =  spendable balances and undistributed custody on Asset Hub
locked         =  in_flight_out + leased + released; every locked unit is
                  held, on custody or on a lease approver's account (§3.3)
teleported     =  the XCM checking account's balance: units teleported to the
                  Coretime chain (§3.1), not spendable on Asset Hub.

where
  in_flight_out  =  deferred transfers whose source has been charged and whose
                    target is not yet credited
  released       =  permanently released units; `excess` and `custodial` are
                    the parts of it booked from returns (§4.4)
  excess         =  the unattributed part of the Parachain Service balance
                    (bad-memo returns, donations, eject surplus above
                    `leased`)
  custodial      =  the claimant-reserved part: a sourced return awaiting its
                    beneficiary
```

---

## 6. Message Protocol

### 6.1 Operations, Correlation

An `OperationId` is unique and never reused. A failed operation is terminal;
a retry creates a new operation with a new id.

A `Delivery`, `Redemption`, `Reclaim` or `Refund` correlates by id: its
`TransferOut` carries the `OperationId` as its `id` field, and a
`TransferFailed { id }` entry points directly at the failed operation.

A recovery operation carries no id on the wire. The Parachain Service keys its
failure entries by service (`ServiceStoreFailed`, `ServiceEjectFailed`,
`ServiceSupervisorFailed`), so it correlates by its target and the class of the
entry. The pallet keeps at most one unsettled operation per target.

### 6.2 Memo Requirements

JAM transfer memos are 128 octets. A voluntary return carries the beneficiary
account in it. The exact layout is to be defined.

---

## 7. References

- [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926):
  burn of all DAO proceeds from JAMKB; no grants, gifts, or below-market loans
- [JAM Gray Paper](https://graypaper.com):
  formal JAM specification (Gavin Wood)
- [Parachain Service on JAM](https://github.com/paritytech/polkadot-sdk/pull/11883)
- [DOT DAO and the need for $JAMKB](https://medium.com/polkadot-network/dot-dao-and-the-need-for-jamkb-a069e72e9728):
  Gavin Wood
- [DOT DAOism under JAM: An Island Story](https://medium.com/polkadot-network/dot-daoism-under-jam-an-island-story-efe0d02ee084):
  Gavin Wood
