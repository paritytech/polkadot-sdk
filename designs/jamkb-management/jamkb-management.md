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

- The flows: supervisor-managed allocation (lease), permanent release (sale)
  and return of funds
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

Initially all JAMKB sits on the Parachain Service. This document calls the
DAO's share of that balance the reserve: the Parachain Service balance minus
the share that backs the Parachain Service's own footprint (the Parachain
Service floor). Asset Hub holds the reserve's 1:1 representation (§3.1). Both
levels track the same cap:

```
Level 2 — Asset Hub
  pallet-assets JAMKB:
    user balances                    — spendable units against the reserve
    `pallet-jamkb` custody           — undistributed and locked (distributed) units

Level 1 — JAM balances:
    reserve                          — the DAO's share of the Parachain Service
                                       balance; backs everything spendable on the Hub
    floor                            — the Parachain Service's own balance (§3.1)
    recipients' supervisor balances  — leases (DAO-controlled, recoverable)
    recipients' regular balances     — permanent releases (outside DAO control)
```

When a balance transfer from Asset Hub to a target JAM service is executed, the
pallet locks the requested amount on Asset Hub. On JAM the same amount moves
from the Parachain Service's reserve to the target JAM service.

The detailed flow below is a governance-executed permanent release (§4.2): one
deferred transfer from the reserve to the target's regular balance. A
supervisor-managed allocation follows the same path, crediting the supervisor
balance instead.

```
━━ Asset Hub block B — execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve_allocation(mode: Permanent, target, amount)
   ▼
Any signed account
   │  execute_allocation(id)
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
   │  transfer reserve → target's regular balance
   │  on failure it writes TransferFailed { id, error } to Asset Hub's
   │  parachain_log
   │  records B's header as Asset Hub's para head

━━ Asset Hub block C — a later block, its lookup-anchor at or past B's accumulation ━

pallet-parachain-system
   │  delivers the verified validation inputs (para head, complete
   │  parachain_log, incoming_transfers)
   ▼
pallet-jamkb
   │  stores the validation inputs

━━ Any later Asset Hub block ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pallet-jamkb
   │  settle(op_id), callable by any party, updates the operation state:
   │  - para head is B or a descendant of B, no TransferFailed for the
   │    operation's id → Confirmed
   │  - a TransferFailed for the operation's id → Failed, the hold is released
   │  - para head not yet at B → the operation stays pending
```

The para head comparison is by hash.

One risk remains: a `TransferFailed` entry can be overwritten in `parachain_log`
(its 64 KiB cap) before Asset Hub has read it. Asset Hub and JAM state then
disagree: the funds were never transferred on JAM, but the units stay locked on
Asset Hub.

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. This asset is managed by `pallet-jamkb`. It
holds the four privileged roles (Owner, Issuer, Admin, Freezer), assigned to it
at initialization. The pallet account has no key, so no external account can
administer the asset.

The full JAMKB cap is minted into the pallet's account. The mint is a one-time
governance-executed runtime call on the Asset Hub.

At bootstrap, JAM services need a service balance to operate, like the Parachain
Service itself. The genesis amount `W` is therefore held after the mint: the
Parachain Service's own share under `Floor` (`genesis_floor`, §3.2), and the
rest under `Released` (§5).

How the Parachain Service's own footprint is funded beyond `genesis_floor`
is not decided. The pallet has no call that changes the floor.

### 3.2 `pallet-jamkb`

A FRAME pallet that manages the JAMKB asset and executes transfer operations
against the Parachain Service. Before any action like a permanent release or a
supervisor-managed allocation is triggered, the tokens are locked in place on
the owner account: a hold is placed on them, and only then does the transfer
execute on the JAM side. On confirmation the tokens stay held in the
pallet's custody (moved there if they were not on the pallet's account); on
failure the hold is released.

The pallet starts inactive. Governance verifies, against public JAM state at a
named block, that the JAM balances match the pallet configuration (`Settings`):
the Parachain Service holds `cap − genesis_released`, and the genesis services
hold the rest. It records the result with `attest(anchor)`; the pallet stores
the anchor. Until attested, every call except `initialize`, `attest`, `pause`,
`resume`, `set_curator` and `set_management_origins` is rejected.

```rust
/// Values set at `initialize`; changes require a governance call and a
/// migration.
struct Settings {
    asset_id: AssetId,                // JAMKB asset Id
    cap: Balance,                     // hard cap, == JAM-side capacity evidence
    parachain_service: ServiceId,     // service Id of Parachain Service
    genesis_floor: Balance,           // the Parachain Service's share of
                                      // `W`, held under `Floor` (§3.1)
    genesis_released: Balance,        // the rest of `W`, held under `Released` (§3.1)
}

/// A JAM block reference. The attestation names the block whose state the
/// reserve was checked against (§3.1). The pallet records it and does not
/// verify it.
struct Anchor {
    timeslot: Timeslot,
    header_hash: Hash,
}

/// A raw storage key of a supervised service (`cleanup_storage`, §4.3).
type Key = BoundedVec<u8, MAX_KEY_LEN>;

/// One approved allocation.
struct Allocation {
    id: AllocationId,
    mode: AllocationMode,             // Lease | Permanent
    target: ServiceId,
    amount: Balance,
    state: AllocationState,
    approver: PalletsOrigin,          // the approving origin: the contract's
                                      // signed origin, or the governance
                                      // origin
    valid_from: BlockNumber,          // execute is rejected before it
    expires_at: BlockNumber,          // set at approval to valid_from +
                                      // valid_for; execute is rejected past
                                      // it; only cancel_allocation stays legal
    conditions: BoundedVec<u8, MAX_CONDITIONS>,  // opaque governance terms,
                                                 // interpreted off-chain only
}

enum AllocationMode { Lease, Permanent }

enum AllocationState {
    Approved,
    Delivering,                       // credit in flight (lease or release);
                                      // on Failed, back to Approved
    Delivered,                        // the delivery settled Confirmed. For a
                                      // release the record is terminal: the
                                      // DAO no longer controls the units
                                      // (§4.2.1). For a lease, reclaim may
                                      // follow (§4.3)
    Reclaiming,                       // reclaim or enforced cleanup in
                                      // progress; mode Lease only
    Closed,                           // cancelled, returned or written off
                                      // (§4.3)
}

/// One cross-system operation (a single JAM-side effect).
struct Operation {
    id: OperationId,                  // unique, never reused; sent as the
                                      // transfer's `id`; `TransferFailed`
                                      // returns the same id (§6.1)
    messages: BoundedVec<UpwardMessage, MAX_KEYS_PER_PAGE>,
                                      // the messages to send, as defined in
                                      // the Parachain Service design §3.3; one
                                      // per operation, except a
                                      // `cleanup_storage` page: one
                                      // `RemoveServiceStorage` per key
    allocation: Option<AllocationId>,
    amount: Option<Balance>,          // None for operations that move no funds
    state: OperationState,
    submitted_at: BlockNumber,        // the sending block B, set when
                                      // parachain-system pulls the message;
                                      // settle reads B's hash from
                                      // frame_system's block hashes (§2)
}

enum OperationState {
    Requested,          // recorded and queued
    Submitted,          // sent: parachain-system pulled it from the queue (§3.4)
    Confirmed,
    Failed,
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
    reserve: Balance,          // the Parachain Service balance available
                               // for distribution (§5)
    excess: Balance,           // unattributed inflows: donations, bad-memo
                               // returns; kept per source service; owner
                               // unknown, disposed by governance (§5)
    custodial: Balance,        // available for claim: sourced returns (§4.4);
                               // owner known
    in_flight_out: Balance,    // outbound deferred transfers sent and not
                               // yet settled: releases and redemptions (§5)
    floor: Balance,            // the Parachain Service's own balance, fixed at
                               // `initialize` (= `genesis_floor`, §3.1);
    released: Balance,         // permanent releases to other services
    leased: Balance,           // active supervisor-balance allocations
    locked: Balance,           // Hub units in locked custody (§5)
}
```

Typed events:

```rust
enum Event {
    /// The genesis attestation is recorded (§3.1).
    Attested { anchor: Anchor },
    AllocationApproved { id: AllocationId, mode: AllocationMode, target: ServiceId,
                         amount: Balance },
    AllocationCancelled { id: AllocationId },
    /// `allocation` is `None` for a redemption (§4.2.2); an enforced-cleanup
    /// step carries the lease's id (§4.3).
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId },
    /// `error` carries the Parachain Service failure.
    OperationFailed { id: OperationId, error: FailureReason },
    /// A return was booked (§4.4): custodial for a named beneficiary, or
    /// excess when the memo is missing or malformed.
    ReturnBooked { source: ServiceId, amount: Balance, class: ReturnClass },
    /// A beneficiary's custodial balance was claimed (§3.2).
    Claimed { beneficiary: AccountId, amount: Balance },
    TargetFrozen { target: ServiceId },
    TargetUnfrozen { target: ServiceId },
    CuratorSet { target: ServiceId, curator: AccountId },
    AllocationWrittenOff { id: AllocationId },
    BudgetGranted { beneficiary: AccountId, amount: Balance },
    ManagementOriginsSet,
    ExcessDisposed { source: ServiceId, amount: Balance },
    Paused,
    Resumed,
}
```

**Policy origin.** Allocations and reclaims are accepted only from a policy
origin: governance origins, or registered management contracts. The allocation
approval fixes everything: mode, target, amount and terms. Execution only
carries out the approved record, so it is open to any signed account. Every
`target` or `dest` naming the Parachain Service is rejected.

The pallet calls. Calls are entry points, operations are the effects: a call
that reaches JAM records an `Operation` with the upward message it sends, using
the name from the Parachain Service design §3.3. Every call returns
`DispatchResult`. Created identifiers are reported through the events.

```rust
/// Origin: governance (Root). Creates the asset and roles, mints `cap` into
/// custody, holds the genesis `W` (§3.1); the pallet stays inactive until attested.
fn initialize(settings: Settings);

/// Origin: governance (Root). Records the genesis attestation against JAM
/// state; enables operations (§3.1).
fn attest(anchor: Anchor);

/// Origin: policy origin. Records `Allocation{state: Approved}` with
/// `expires_at = valid_from + valid_for`. `valid_from` is `None` for the
/// current block; a management contract passes `None` and executes in the
/// same invocation. A lease is rejected if its target is recorded frozen or
/// already has a lease that is not `Closed`.
fn approve_allocation(mode: AllocationMode, target: ServiceId, amount: Balance,
                      valid_from: Option<BlockNumber>, valid_for: BlockNumber,
                      conditions: BoundedVec<u8, MAX_CONDITIONS>);

/// Origin: the allocation's approver, or governance. Cancels an allocation
/// still in `Approved`; the allocation state moves to `Closed`.
fn cancel_allocation(id: AllocationId);

/// Origin: governance (Root). Registers the curator for a target's recovery
/// (§4.3). The curator is a bounty curator: appointment with deposit, fee and
/// slashing live in `pallet-bounties`; the pallet stores only the account.
fn set_curator(target: ServiceId, curator: AccountId);

/// Origin: governance (Root). Registers the management contracts accepted as
/// the policy origin, each with its amount cap. The call replaces the
/// previous set. The cap bounds each approval. An allocation approved by a
/// removed
/// contract stays executable; governance can cancel it.
fn set_management_origins(contracts: BoundedVec<(AccountId, Balance), MAX_POLICY_CONTRACTS>);

/// Origin: governance (Root). Sets and clears the pallet pause flag, checked
/// by every entry point except `settle` (§2). Messages already queued are
/// still processed.
fn pause();
fn resume();

/// Origin: any signed account. Legal only while the allocation is `Approved`,
/// at or past `valid_from` and not past `expires_at`. Each execution creates
/// a new transfer operation; a failed one is terminal (§6.1). Locks the units
/// and records an operation sending `TransferOut` to the target. The transfer
/// credits the supervisor balance and is plain (deferred = None) for a lease;
/// it credits the regular balance and is deferred for a permanent release
/// (§4.1, §4.2).
fn execute_allocation(id: AllocationId);

/// Origin: the target's curator.
/// Foreign `SetServiceCode` of the target to a preimage-free hash (for example
/// zero). The target never executes again, so it cannot take more state
/// footprint. Runs before `cleanup_storage` on a non-cooperating target. The
/// call needs a `SetServiceCode` upward message and a
/// `ServiceCodeFailed { service, error }` entry, which the Parachain Service
/// does not provide yet.
fn freeze_target(target: ServiceId);

/// Origin: the target's curator.
/// Restores the frozen target's original code.
fn unfreeze_target(target: ServiceId);

/// Origin: the target's curator.
/// Deletes keys from the supervised target's own storage: one bounded page,
/// each key sent as `RemoveServiceStorage { service, key }`.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Origin: the target's curator.
/// Releases a previously solicited preimage of the target.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Origin: the target's curator.
/// Destroys the emptied supervised target, crediting its balances to the
/// Parachain Service (`EjectService`). Its Confirmed settle moves the lease
/// to `Closed`; a Failed settle leaves the lease `Reclaiming` (§4.3). The
/// swept supervisor balance counts as the lease return up to `leased`; any
/// surplus books as excess (§5). That booking needs the `Ejected` amounts,
/// which the Parachain Service does not provide yet.
fn eject_target(target: ServiceId);

/// Origin: the target's curator, or the policy origin. Releases the
/// supervised target to itself (`SetServiceSupervisor`). Rejected while the
/// target is recorded frozen: a frozen, self-supervised service is
/// unrecoverable by anyone, permanently. The pallet queues this operation
/// itself when a full reclaim settles Confirmed and the target is not
/// recorded frozen (§4.3); the call re-issues it after a failure, and ends
/// the enforced cleanup on the RESTORE exit.
fn unsupervise(target: ServiceId);

/// Origin: policy origin. Takes the full leased balance back: debits the
/// lease target's supervisor balance and credits the reserve, a plain
/// `TransferOut` (deferred = None). Legal from `Delivered`, and from
/// `Reclaiming` with no pending operation (§4.3 RESTORE). A confirmed reclaim
/// ends the lease.
fn reclaim(id: AllocationId);

/// Origin: governance (Root). Transfers `amount` of undistributed units from
/// the pallet's custody to `beneficiary` on Asset Hub: a budget for a policy
/// adapter (§1).
fn grant(beneficiary: AccountId, amount: Balance);

/// Origin: any holder. Places a hold on the caller's units (§3.3); records an
/// operation sending a deferred `TransferOut` to `dest`'s regular balance
/// (§4.2.2).
fn redeem(amount: Balance, dest: ServiceId);

/// Origin: any signed account. Claims the voluntarily returned balance to
/// `beneficiary`'s account (§4.4).
fn claim(beneficiary: AccountId);

/// Origin: governance (Root). Disposes an excess amount (§5): refunds it to
/// its source service by a TransferOut queued in the same dispatch, or
/// releases `amount` of the `Released` hold back to undistributed custody.
fn dispose_excess(source: ServiceId, amount: Balance, refund: bool);

/// Origin: any signed account. Updates the operation state per §2. For an
/// operation already marked `Failed` at delivery (§3.4), it releases the hold
/// and updates the allocation.
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
/// The hold classes. An in-flight hold is recorded under its destination
/// class (§4.1, §4.2).
enum HoldReason {
    /// The Parachain Service's `genesis_floor` (§3.1).
    #[codec(index = 0)]
    Floor,
    /// Units backing an active lease (§4.1).
    #[codec(index = 1)]
    Leased,
    /// Units backing outstanding permanent releases (§4.2).
    #[codec(index = 2)]
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

For each operation it records, the pallet appends the message to
`PendingOperations` (the per-block send queue) and sets the operation
`Requested`. The parachain-system pallet takes the queue through the source
trait and calls `send_upward_message` for each message; the runtime `Config`
names `pallet-jamkb` as the only provider of the message variants it sends.

The generic pallet's inherent verifies `(anchor, proof, para head,
parachain_log, incoming_transfers)` against the anchor's posterior state-root.
The pallet stores what the inherent verified. On each delivery it matches the
failure entries against its `Submitted` operations and marks the matches
`Failed`; `settle(op_id)` finalizes the holds and the allocation state. The
64 KiB log cap bounds the matching work.

---

## 4. Allocation Protocols

### 4.1 Lease (Supervisor-Managed Allocation)

A lease is a token transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor.

```
Phase 1: Approve      The policy origin approves Allocation{mode: Lease, target, amount}.
Phase 2: Execute      Any signed account calls `execute_allocation(id)` (§3.2). The
                      pallet holds `amount` in its custody under `Leased` and
                      queues a TransferOut crediting the target's supervisor
                      balance (§3.4).
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the held units stay held under `Leased`,
                      backing the lease (§5); the allocation moves to Delivered.
                      Failed: JAM rejected the transfer; the hold is released
                      (§5); the allocation is back to Approved.
```

### 4.2 Permanent Release

A permanent release is a token transfer to the target service's regular balance.

Units reach a regular balance by two routes: the DAO releases them to a named
service (§4.2.1), or a holder releases their own units (§4.2.2). A market sale
uses the second route: the DAO grants an adapter a budget (`grant`,
§3.2), the adapter sells the units on the Hub, and the buyer releases them.

#### 4.2.1 Governance-initiated Release

Governance releases units from DAO custody.

```
Phase 1: Approve      The policy origin approves Allocation{mode: Permanent, target, amount}.
Phase 2: Execute      Any signed account calls `execute_allocation(id)` (§3.2). The
                      pallet holds `amount` in its custody under `Released`
                      and queues a TransferOut crediting the target's regular
                      balance (§3.4).
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
                      places a hold on the holder's units (§3.1) and queues a
                      TransferOut crediting the target's regular balance (§3.4).
Phase 2: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4).
Phase 3: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the held units move into
                      the pallet's custody, arriving held under `Released` (§3.3).
                      Failed: JAM rejected the transfer; the hold on the
                      holder's units is released.
```

### 4.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reclaim      On the policy origin's call, the pallet queues a
                      TransferOut debiting the target's supervisor balance,
                      amount = the full lease (§3.4); the lease moves to
                      Reclaiming.
Phase 3: Submit       pallet-parachain-system sends the TransferOut via
                      `send_upward_message` (§3.4). It fails if, after the
                      debit, balance + supervisor_balance < the threshold
                      balance.
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the `Leased` hold is released (§5); the
                      allocation moves to Closed; the pallet queues unsupervise(target),
                      handing the target back to self-supervision.
                      Failed: JAM rejected the transfer; the lease returns
                      to Delivered.
```

Full return, non-cooperative. Entered when the lease has ended, the target has
not freed the footprint, and the flow above failed.

Supervision gives the DAO full power over the target, including cleaning its
state and ejecting it:

```
Phase 1: Freeze       The curator submits `freeze_target` (§3.2); the lease
                      moves to Reclaiming.
Phase 3: Cleanup      The curator submits `cleanup_storage` pages (§3.2),
                      and `forget_preimage` for every preimage except the
                      target's code preimage.
                      The exit fork:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; `unfreeze_target`
                          restores the original code hash; unsupervise(target)
                          ends the enforced cleanup, leaving the target
                          self-supervised.
                      (b) TERMINATE: continue below.
Phase 3b: Forget      Discard the code preimage: `forget_preimage`; eject fails
                      `NotEmpty` until the preimage is expunged.
Phase 4: Eject        The curator submits `eject_target` (§3.2).
Phase 5: Confirm      The eject settles by a `settle(op_id)` call on the
                      pallet (§6.1); the lease-return hold is released (§5);
                      the lease moves to Closed.
```


- Storage keys are not recoverable from state and must be tracked externally.
  Without keys, cleanup is impossible.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service reserve.

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
cap  =  unlocked + locked                                                (Hub view)
     =  reserve + floor + in_flight_out + leased + released              (JAM view)

unlocked       =  user balances + undistributed custody (§3.1)  =  reserve
locked         =  floor + in_flight_out + leased + released; every
                  locked unit is a held unit (§3.1)

where
  reserve        =  the Parachain Service balance above the floor
                    (`genesis_floor`, §3.1; equals `Totals.floor`), minus
                    excess and custodial
  in_flight_out  =  deferred transfers where the source has been charged but
                    the target not yet credited; the funds sit in no service
                    balance. Counted by `Totals.in_flight_out` as the
                    outbound deferred transfers sent and not yet settled.
  released       =  outstanding releases, net of matched returns (§4.4)
  excess         =  the unattributed part of the Parachain Service balance
                    (bad-memo returns, donations, eject surplus above
                    `leased`), counted by `Totals.excess` (§3.2); an unrecorded
                    return stays outside the count and unusable (§4.4).
                    Owner unknown; outside the identity; disposed by governance
  custodial      =  the claimant-reserved part (a sourced return awaiting
                    its beneficiary), counted by `Totals.custodial` (§3.2).
                    Owner known; reserved for its claimant; never converted
                    into backing
```

---

## 6. Message Protocol

### 6.1 Operations, Correlation

An `OperationId` is unique and never reused. The transfer sent to JAM carries
the `OperationId` as its `id` field. On failure, `TransferFailed { id }`
returns the same id, so the entry points directly at the failed operation. A
failed operation is terminal; a new execution creates a new operation with a
new id.

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
