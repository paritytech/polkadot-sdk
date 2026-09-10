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
   - 4.1
     [Lease (Supervisor-Managed Allocation)](#41-lease-supervisor-managed-allocation)
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

- The flows: supervisor-managed allocation (lease), permanent release (sell) and
  funds return
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

Initially all JAMKB sits on the Parachain Service; this document calls that
balance the reserve. Asset Hub holds its 1:1 representation (§3.1). Both levels
track the same cap:

```
Level 2 — Asset Hub
  pallet-assets JAMKB:
    user balances                    — spendable units against the reserve
    `pallet-jamkb` custody           — undistributed and locked (distributed) units

Level 1 — JAM balances:
    reserve                          — a Parachain Service balance;
                                       backs everything spendable on the Hub
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
Registered operator
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
   │  - para head at or past B, no TransferFailed for the operation's id → Confirmed
   │  - a TransferFailed for the operation's id → Failed, the hold is released
   │  - para head still below B → the operation stays pending
```

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
Parachain Service's own slice under `Floor` (`genesis_floor`, §3.2), and the
rest under Released (§5). How the Parachain Service's own footprint is funded
beyond this genesis slice is not decided; the pallet has no call that changes
the floor.

### 3.2 `pallet-jamkb`

A FRAME pallet that manages the JAMKB asset and executes transfer operations
against the Parachain Service. Before any action like a permanent release or a
supervisor-managed allocation is triggered, the tokens are locked in place on
the owner account: a hold is placed on them, and only then does the transfer
execute on the JAM side. On confirmation the tokens stay held permanently in the
pallet's custody (moved there if they were not on the pallet's account); on
failure the hold is released.

```rust
/// Values set at `initialize`; changes require a governance call and a
/// migration.
struct Settings {
    asset_id: AssetId,                // JAMKB asset Id
    cap: Balance,                     // hard cap, == JAM-side capacity evidence
    parachain_service: ServiceId,     // service Id of Parachain Service
    genesis_floor: Balance,           // the genesis `W` slice held under `Floor` (§3.1)
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

/// One governance-approved allocation.
struct Allocation {
    id: AllocationId,
    mode: AllocationMode,             // Lease | Permanent
    target: ServiceId,
    amount: Balance,
    state: AllocationState,
    conditions: BoundedVec<u8, MAX_CONDITIONS>,  // opaque governance terms,
                                                 // interpreted off-chain only
}

enum AllocationMode { Lease, Permanent }

enum AllocationState {
    Approved,
    Delivering(OperationId),          // credit in flight (lease or release);
                                      // on Failed, back to Approved
    Active,                           // lease live
    Released,                         // release confirmed; the DAO no longer
                                      // controls the units
    Reclaiming(OperationId),          // reclaim or enforced cleanup in progress
    Closed,                           // returned or written off
}

/// One cross-system operation (a single JAM-side effect).
struct Operation {
    id: OperationId,                  // unique, never reused; carried as the
                                      // wire transfer id (§6.1)
    message: UpwardMessage,           // the message to emit, as the Parachain
                                      // Service design §3.3 spells it;
                                      // `cleanup_storage` records a key page,
                                      // emitted as one `RemoveServiceStorage`
                                      // per key
    allocation: Option<AllocationId>,
    amount: Balance,
    state: OperationState,
    submitted_at: BlockNumber,        // the emitting block B; settle compares
                                      // the para head against it (§2)
}

enum OperationState {
    Requested,          // recorded and queued; not yet emitted
    Submitted,          // emitted: parachain-system pulled it from the queue (§3.4)
    Confirmed,
    Failed,
}
```

Aggregates maintained for reporting and the conservation check (§5):

```rust
struct Totals {
    reserve: Balance,          // attributed reserve = JAM reserve balance
                               // − excess − custodial, mirrored at the
                               // attestation anchor
    excess: Balance,           // unattributed inflows: donations, stray
                               // sweeps, bad-memo returns; owner unknown,
                               // disposed by governance (§5)
    custodial: Balance,        // claimant-reserved inflows: §4.3 payouts and
                               // sourced returns; owner known, never converted
                               // into backing (§5)
    in_flight_out: Balance,    // JAM-debited, not yet credited at destination (§5)
    floor: Balance,            // the Parachain Service's own slice, fixed at
                               // `initialize` (= `genesis_floor`, §3.1); on
                               // the PS account, DAO-controlled, never netted
                               // by returns
    released: Balance,         // outstanding permanent releases to other
                               // services (net of attributed returns of
                               // released units); excludes the floor
    leased: Balance,           // active supervisor-balance allocations
    locked: Balance,           // Hub units in locked custody
                               // = floor + in_flight_out + leased + released (§5)
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
    /// `allocation` is `None` for a redemption (§4.2.2) or an enforced-cleanup
    /// step (§4.3).
    OperationSubmitted { id: OperationId, allocation: Option<AllocationId> },
    OperationConfirmed { id: OperationId },
    OperationFailed { id: OperationId },
    ReturnCredited { beneficiary: AccountId, amount: Balance },
    Paused,
    Resumed,
}
```

**Policy origin.** Allocations and reclaims are accepted only from a policy
origin set by governance: the DAO itself, or management contracts, each with a
governance-set amount cap. A management contract registered as an operator
approves and executes an allocation in one invocation.

The pallet calls. Calls are entry points, operations are the effects: a call
that reaches JAM records an `Operation` (§3.2) with the upward message it emits,
named as the Parachain Service design §3.3 spells it. Every call returns
`DispatchResult`. Created identifiers are reported through the events; failures
through the pallet's `Error` enum, which is not specified here.

```rust
/// Origin: governance (Root). Creates the asset and roles, mints `cap` into
/// custody, holds the genesis `W` (§3.1); the pallet stays inactive until attested.
fn initialize(settings: Settings);

/// Origin: governance (Root). Records the genesis attestation against JAM
/// state; enables operations (§3.1).
fn attest(anchor: Anchor);

/// Origin: policy origin. Records `Allocation{state: Approved}`. A `target`
/// naming the Parachain Service is rejected.
fn approve_allocation(mode: AllocationMode, target: ServiceId, amount: Balance,
                      conditions: BoundedVec<u8, MAX_CONDITIONS>);

/// Origin: policy origin. Cancels an allocation still in `Approved`; the
/// record closes without effect.
fn cancel_allocation(id: AllocationId);

/// Origin: governance (Root). Registers the operator accounts.
fn set_operators(accounts: BoundedVec<AccountId, MAX_OPERATORS>);

/// Origin: governance (Root). Registers the management contracts accepted as
/// the policy origin, each with its amount cap; replaces the previous set.
fn set_management_origins(contracts: BoundedVec<(AccountId, Balance), MAX_POLICY_CONTRACTS>);

/// Origin: governance (Root). Sets and clears the pallet pause flag, checked
/// by every entry point; a Root `force_asset_status` freeze remains the
/// separate asset-wide brake.
fn pause();
fn resume();

/// Origin: operator. Legal only while the allocation is `Approved`; locks the
/// units and records an operation emitting `TransferOut` to the target. The
/// transfer credits the supervisor balance and is plain (deferred = None) for
/// a lease; it credits the regular balance and is deferred (memo, gas) for a
/// release (§4.1, §4.2).
fn execute_allocation(id: AllocationId);

/// Origin of the seven calls below: operator. Each records one operation (§4.3).

/// Foreign `SetCode` of the target to a preimage-free hash (e.g. zero; GP Ω_U
/// performs no availability check). The target never executes again: no
/// write, no solicit, no code or supervisor change. Runs before
/// `cleanup_storage` on a non-cooperating target.
fn freeze_target(target: ServiceId);

/// Deletes keys from the supervised target's own storage: one bounded page,
/// each key emitted as `RemoveServiceStorage { service, key }`. Stateless and
/// idempotent on the Parachain Service side; pagination is operator-side; the
/// on-chain `items` counter shows when the storage is empty.
fn cleanup_storage(target: ServiceId, keys: BoundedVec<Key, MAX_KEYS_PER_PAGE>);

/// Releases a previously solicited preimage of the target:
/// `Forget { target: Target::Service, hash, len }`.
fn forget_preimage(target: ServiceId, hash: Hash, len: u32);

/// Takes the leased balance back: debits the target's supervisor balance and
/// credits the reserve, a plain `TransferOut` (deferred = None). The full
/// lease amount ends the lease; a partial amount resizes it.
fn reclaim(target: ServiceId, amount: Balance);

/// Deferred payout of the target's regular balance during the enforced
/// cleanup: debits `target` and credits `dest`. `TransferOut` fixes `amount`
/// at emission, so credits arriving later are missed; `eject_return` sweeps
/// the residue. Settlement needs the `PaidOut { id, amount }` echo, which the
/// Parachain Service does not provide yet.
fn payout_regular(target: ServiceId, dest: ServiceId);

/// Destroys the emptied supervised target, crediting its balances to the
/// Parachain Service (`EjectService`).
fn eject_return(target: ServiceId);

/// Hands the supervised target to another supervisor, or to itself to set it
/// free (`SetServiceSupervisor`). Handover to the target itself is legal only
/// if its codehash resolves to an available preimage; a frozen,
/// self-supervised service is unrecoverable by anyone, permanently.
fn hand_over_supervision(target: ServiceId, new_supervisor: ServiceId);

/// Origin: any holder. Places a hold on the caller's units (§3.1); records an
/// operation emitting a deferred `TransferOut` to `dest`'s regular balance
/// (§4.2.2).
fn redeem(amount: Balance, dest: ServiceId);

/// Origin: any signed account. Updates the operation state per §2.
fn settle(op_id: OperationId);
```

### 3.3 pallet-assets access

`pallet-assets` holds JAMKB. The pallet holds the four roles and administers the
asset through the runtime-internal fungibles traits: `transfer`, `hold`,
`release`, `transfer_on_hold` and `mint`. No administration precompile is
needed. Asset-wide status changes need no role: governance uses
`force_asset_status` (Root). Policy adapters move their budgets through the
existing ERC20 precompile.

**Hold rules.** The hold reason is declared in the runtime as one closed set
with fixed codec indices. All hold operations use exact precision. `burn_held`
is never used: it reduces total issuance, which the cap forbids (§3.1). Holds
never touch the asset's minimum balance, so a full-balance redeem moves the
final `min_balance` by a plain transfer. The hold classes:

```rust
/// The hold classes (§3.1). An in-flight hold is recorded under its
/// destination class (§4.1, §4.2).
enum HoldReason {
    /// The Parachain Service's genesis slice (§3.1).
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

The pallet holds no code that reaches the Parachain Service directly, so it is
layered on a generic transport mechanism. The planned
[cumulus-on-jam](../cumulus-on-jam/cumulus-on-jam.md) §11 `validate_block`
rework is expected to provide it, probably by extending `parachain-system` into
a generic AH→JAM transport pallet. The generic pallet would own the outbound
upward messages, their emission through `send_upward_message` inside
`jam_validate_block`, and the inherent that delivers the validation inputs.

#### The parachain-system pallet requirements

The design of this pallet is out of scope of this document. The requirements
below are what the pallet needs from it to operate.

The Parachain Service restricts `TransferOut` and its sibling upward messages:
they are accepted only from the Asset Hub parachain. Such `UpwardMessage`
variants have no public push API in the parachain-system pallet. The generic
pallet pulls each restricted variant class from one provider named in the
runtime `Config`, following the `XcmpMessageSource` pattern.

The generic pallet exposes the inherent data (`parachain_log` entries,
`incoming_transfers`) to pallets as validation inputs.

#### What the pallet owns

For each operation it records, the pallet appends the message to
`PendingOperations` (the per-block emission queue) and sets the operation
`Requested`. The parachain-system pallet takes the queue through the source
trait and calls `send_upward_message` for each message; the runtime `Config`
names `pallet-jamkb` as the only `TransferOut` provider.

The generic pallet's inherent verifies `(anchor, proof, para head,
parachain_log, incoming_transfers)` against the anchor's posterior state-root.
The pallet stores what the inherent verified and consumes it via
`settle(op_id)`.

---

## 4. Allocation Protocols

### 4.1 Lease (Supervisor-Managed Allocation)

A lease is a token transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor.

```
Phase 1: Approve      The policy origin approves Allocation{mode: Lease, target, amount}.
Phase 2: Execute      The operator calls `execute_allocation(id)` (§3.2). The
                      pallet holds `amount` in its custody under `Leased` and
                      queues a TransferOut crediting the target's supervisor
                      balance (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a plain `transfer` (reserve → target's supervisor
                      balance; deferred = None).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the held units stay held under `Leased`,
                      backing the lease (§5).
                      Failed: JAM rejected the transfer; the hold is released (§5).
```

### 4.2 Permanent Release

A permanent release is a token transfer to the target service's regular balance.

#### 4.2.1 Governance-initiated Release

Governance releases units from DAO custody.

```
Phase 1: Approve      The policy origin approves Allocation{mode: Permanent, target, amount}.
Phase 2: Execute      The operator calls `execute_allocation(id)` (§3.2). The
                      pallet holds `amount` in its custody under `Released`
                      and queues a TransferOut crediting the target's regular
                      balance (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a deferred `transfer` (reserve → target's regular
                      balance; memo §6.2).
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units stay held
                      under `Released`.
                      Failed: JAM rejected the transfer; the hold is released.
```

#### 4.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service,
bypassing governance:

```
Phase 1: Lock         Holder calls `redeem(amount, dest)` (§3.2). The pallet
                      places a hold on the holder's units (§3.1) and queues a
                      TransferOut crediting the target's regular balance (§3.4).
Phase 2: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a deferred `transfer` (reserve → the target's regular
                      balance; memo §6.2).
Phase 3: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the held units move into
                      the pallet's custody, arriving held under `Released` (§3.1).
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
                      amount = the full lease (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service
                      executes it as a plain `transfer` (target's supervisor
                      balance → reserve; deferred = None). It fails if the service
                      balance + supervisor_balance < threshold balance.
Phase 4: Confirm      Any party can call `settle(op_id)` on the pallet (§2).
                      The output:
                      Confirmed: the `Leased` hold is released (§5); the
                      target is handed back to self-supervision
                      (hand_over_supervision(target, target)).
                      Failed: JAM rejected the transfer; the lease stays Active.
```

Full return, non-cooperative. Entered when the lease has ended and the target
has not freed the footprint and the flow above failed.

Supervision gives the DAO full power over the target, including cleaning its
state and ejecting it. Exercising that power breaks the expectation that
services are unstoppable. An alternative could avoid it: collateral is set at
lease allocation, and an unreturned lease is treated as a sale. The collateral
is charged and the target keeps running. The drawback is that funds equal to the
token sale price need to be locked for the lease duration.

On the other side, if the enforced-cleanup direction is taken to release the
tokens, the gap of the lacking storage keys remains (see the storage-keys note
below). The flow with enforced cleanup:

```
Phase 1: Freeze       An operator submits `freeze_target`. Parachain Service issues
                      a foreign `SetCode` to a 32-byte preimage-free hash
                      (e.g. zero).
Phase 2: Drain        Confirm all outbound transfers to the target have
                      settled. The pallet rejects new operations.
Phase 3: Cleanup      An operator submits `cleanup_storage` pages; the Parachain
                      Service executes `RemoveServiceStorage { target, key }`
                      for each key. An operator submits `forget_preimage(hash, len)`
                      for every preimage except the target's code preimage.
                      The exit fork:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; unfreeze via `SetCode`
                          restoring the original code hash;
                          hand_over_supervision(target, target) ends the
                          enforced cleanup, leaving the target self-supervised.
                      (b) TERMINATE: continue below.
Phase 3b: Forget      Discard the code preimage: `forget_preimage`; eject fails
                      `NotEmpty` until the preimage is expunged.
Phase 4: Payout       An operator submits `payout_regular`; the Parachain Service
                      defers the target's regular balance back to the Parachain
                      Service itself. The amount books as custodial with its
                      provenance: owner-claimable (§5).
Phase 5: Confirm      The payout settles by a `settle(op_id)` call on the
                      pallet. The eject may be submitted only after this
                      settles Confirmed.
Phase 6: Eject        An operator submits `eject_return`; `eject(target)` sweeps
                      the remaining balances to the Parachain Service. The
                      swept supervisor balance counts as the lease return up
                      to `leased`; any surplus (third-party credits, payout
                      dust) books as excess (§5).
Phase 7: Confirm      The eject settles the same way; the lease-return hold
                      is released (§5).
```

- A frozen target cannot solicit or write. Freeze prevents the target from
  pinning more footprint. A frozen self-supervised service is permanently
  unrecoverable: it can neither be unfrozen nor ejected, as both require a
  supervisor other than the target itself.

- Storage keys are not recoverable from state and must be tracked externally.
  Without keys, cleanup is impossible.

### 4.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service reserve.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = return attribution, §6.2); the Parachain
                      Service queues it in `incoming_transfers`.
Phase 2: Confirm      The pallet records the return from the stored
                      validation inputs (§3.4); any party can trigger it.
                      The output:
                      Valid memo (§6.2): the named Asset Hub account is
                      credited from held custody, in one `transfer_on_hold`
                      (§3.1).
                      Uncreditable beneficiary (below the minimum balance, or
                      the account no longer exists): the credit is classified
                      as custodial with its provenance; crediting it requires an
                      operator action once the beneficiary can receive.
                      Missing or malformed memo: funds are classified as excess (§5),
                      recorded with its source; a refund to the source requires an
                      operator action.
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
  reserve        =  the Parachain Service balance above the floor (the genesis
                    slice for the Parachain Service's own footprint, fixed at
                    `initialize`, §3.1; equals `Totals.floor`), minus excess
                    and custodial
  in_flight_out  =  deferred transfers where the source
                    has been charged but the target not yet credited; the funds
                    sit in no service balance.
  released       =  outstanding releases, net of matched returns (§4.4)
  excess         =  the unattributed slice of the Parachain Service balance
                    (bad-memo returns, donations, reclaim surplus above
                    `leased`), counted by `Totals.excess` (§3.2); an unrecorded
                    return enters the count only when governance books it.
                    Owner unknown; outside the identity; disposed by governance
  custodial      =  the claimant-reserved slice (a payout awaiting collection,
                    a sourced return), counted by
                    `Totals.custodial` (§3.2). Owner known; reserved for its
                    claimant; never converted into backing
```

---

## 6. Message Protocol

### 6.1 Operations, Correlation

An `OperationId` is unique and never reused. The transfer sent to JAM carries
the `OperationId` as its `id` field. On failure, `TransferFailed { id }`
returns the same id, so the entry points directly at the failed operation.

### 6.2 Memo Requirements

JAM transfer memos are 128 octets. A voluntary return carries the beneficiary
account in it. The exact layout is to be defined.

---

## 7. References

- [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926): Burn of all DAO proceeds from JAMKB; no grants, gifts, or below-market loans
- [JAM Gray Paper](https://graypaper.com): Formal JAM specification (Gavin Wood)
- [Parachain Service on JAM](https://github.com/paritytech/polkadot-sdk/pull/11883)
- [DOT DAO and the need for $JAMKB](https://medium.com/polkadot-network/dot-dao-and-the-need-for-jamkb-a069e72e9728): Gavin Wood
- [DOT DAOism under JAM: An Island Story](https://medium.com/polkadot-network/dot-daoism-under-jam-an-island-story-efe0d02ee084): Gavin Wood
