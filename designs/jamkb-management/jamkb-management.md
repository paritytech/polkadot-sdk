# JAMKB Management on Asset Hub

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Asset Hub Components](#3-asset-hub-components)
   - 3.1 [The JAMKB Asset](#31-the-jamkb-asset)
   - 3.2 [Manager Contract](#32-manager-contract)
   - 3.3 [pallet-assets precompile](#33-pallet-assets-precompile)
   - 3.4 [`pallet-jamkb` (JAMKB Operations on the Generic AH→JAM Transport)](#34-pallet-jamkb-jamkb-operations-on-the-generic-ahjam-transport)
5. [Allocation Protocols](#5-allocation-protocols)
   - 5.1 [Lease (Supervisor-Managed Allocation)](#51-lease-supervisor-managed-allocation)
   - 5.2 [Permanent Release](#52-permanent-release)
   - 5.3 [Lease Return](#53-lease-return)
   - 5.4 [Voluntary Return](#54-voluntary-return)
6. [Cap & Backing Accounting](#6-cap--backing-accounting)
7. [Message Protocol](#7-message-protocol)
   - 7.1 [Operations, Correlation](#71-operations-correlation)
   - 7.2 [Memo Requirements](#72-memo-requirements)
8. [References](#8-references)

---

## 1. Overview

This document describes the architecture of JAMKB management on Asset Hub. JAMKB is JAM's resource-access token for state footprint. A JAM service may keep as much state as its balance covers. Asset Hub carries a 1:1 representation of the token, where it is managed, sold and leased.

### Scope

This document covers:

- The flows: lease, permanent release and funds return
- The components they use: the JAMKB asset, the manager contract, the
  administration precompile, jamkb pallet

This document does not cover economic policy: how JAMKB is priced, sold or distributed.

---

## 2. Architecture Overview

A JAM service has two balances: a regular balance and a supervisor balance. Both back the service's state footprint. The service can transfer its regular balance, but the supervisor balance can be transferred only by the effective supervisor. In this design the supervisor is the Parachain Service.

Initially all JAMKB sits on the Parachain Service; this document calls that balance the reserve. Asset Hub holds its 1:1 representation (§3.1). Both levels track the same cap:

```
Level 2 — Asset Hub
  pallet-assets JAMKB:
    user balances      — spendable units against the reserve
    manager custody    — undistributed and locked (distributed) units

Level 1 — JAM balances:
    reserve                          — a Parachain Service balance;
                                       backs everything spendable on the Hub
    recipients' supervisor balances  — leases (DAO-controlled, recoverable)
    recipients' regular balances     — permanent releases (outside DAO control)
```

When a balance transfer from Asset Hub to a target JAM service is executed, the manager locks the requested amount on Asset Hub. On JAM the same amount moves from the Parachain Service's reserve to the target JAM service.

The detailed flow below is a governance-executed permanent release (§5.2): one deferred transfer from the reserve to the target's regular balance. A lease follows the same path, crediting the supervisor balance instead.

```
━━ Asset Hub block B — execution ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Governance (Root)
   │  approve Allocation{mode: Permanent, target, amount}
   ▼
Manager contract (run on pallet-revive)
   │  lock the units (§3.1) and record the operation
   ▼
pallet-jamkb
   │  request_operation(op): a TransferOut appended to PendingOperations
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

Manager contract
   │  settle(op_id), callable by any party, updates the operation state:
   │  - para head at or past B, no TransferFailed for the operation's id → Confirmed
   │  - a TransferFailed for the operation's id → Failed, unlock funds
   │  - para head still below B → the operation stays pending
```

Worth noting here is one risk: a `TransferFailed` entry can be overwritten in `parachain_log` (its 64 KiB cap) before Asset Hub has read it. Asset Hub and JAM state then disagree: the funds were never transferred on JAM, but the units stay locked on Asset Hub.

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's balance on the Parachain Service. This asset is managed by manager contract. It holds the four privileged roles (Owner, Issuer, Admin, Freezer), assigned to it at initialization.

The full JAMKB cap is minted into the manager contract's account. The mint is a one-time governance-executed runtime call on the Asset Hub.

At bootstrap, JAM services need a service balance to operate, like the Parachain Service itself. The amount `W` assigned to JAM services at genesis is therefore locked on the Asset Hub side after the mint and counted in `released` (§6).

### 3.2 Manager Contract

A pallet-revive contract that manages the JAMKB asset and executes transfer operations against the Parachain Service.

```rust
/// Configuration fixed at initialization; changes require governance + migration.
struct Config {
    asset_id: AssetId,
    cap: Balance,                     // hard cap, == JAM-side capacity evidence
    parachain_service: ServiceId,     // the JAM-side executor & supervisor
    /// The reserve is a tracked line within the Parachain Service's own balance;
    /// the floor below which the manager never draws it (it collateralizes hosted
    /// platform state).
    segregation_floor: Balance,
}

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
    Released,                         // release confirmed; terminal for control
    Reclaiming(OperationId),          // reassignment / wind-down in progress
    Closed,                           // returned or written off
}

/// One cross-system operation (a single JAM-side effect).
struct Operation {
    id: OperationId,                  // unique, never reused; carried as the
                                      // wire transfer id (§7.1)
    kind: OperationKind,
    allocation: Option<AllocationId>,
    amount: Balance,
    state: OperationState,
    submitted_at: BlockNumber,        // the emitting block B; settle compares
                                      // the para head against it (§2)
}

/// Naming: `target` is the service an operation acts upon; `dest` is the
/// transfer-destination field, spelled as in the Parachain Service `TransferOut` ABI.
/// `PayoutRegular` needs both: it drains `target` and pays `dest`.
enum OperationKind {
    /// Maps to `UpwardMessage::TransferOut`, which already carries per-side
    /// supervisor-balance selectors, an optional foreign source, and the
    /// deferred mode — Reassign and PayoutRegular below are
    /// parameterizations of that same verb, not new Parachain Service messages.
    TransferOut { dest: ServiceId, dest_supervisor: bool },
    /// Foreign SetCode of the target to a preimage-free hash (e.g. zero; GP
    /// Ω_U performs no availability check). The target never executes: no
    /// write, solicit, re-code, or re-supervise — closes the solicit-pinning
    /// race. Costs the target no footprint (an available-stub variant
    /// would pin 101+len and block eject behind a ~32 h expunge) and is
    /// reversible while the real code preimage is retained (§5.3 restore
    /// exit). Mandatory before Cleanup against a non-cooperating target. §5.3.
    FreezeTarget { target: ServiceId },
    /// Guard: handover to the target itself is legal only if its codehash
    /// resolves to an available preimage — a frozen, self-supervised service
    /// is unrecoverable by anyone, permanently (§5.3 exit rule).
    HandOverSupervision { target: ServiceId, new_supervisor: ServiceId },
    Reassign { target: ServiceId, amount: Balance },
    /// One bounded page of operator-supplied storage keys, each replayed as
    /// `RemoveServiceStorage { service, key }` (Parachain Service design §3.3).
    /// Stateless & idempotent on the Parachain Service side; pagination (list custody,
    /// cursor, retries, completion) is entirely operator-side; the on-chain
    /// `items` counter is the completion oracle. §5.3.
    CleanupStorage { target: ServiceId, keys: Vec<Vec<u8>> },
    /// Foreign two-step forget of one target preimage —
    /// `Forget { target: Target::Service, hash, len }` (Parachain Service design §3.3). (hash, len) of
    /// Provided preimages are recoverable from state (hash the stored value);
    /// unprovided solicits need execution capture (§5.3 storage-keys note).
    /// The target's own code preimage is forgotten last, and only on the
    /// terminate exit (§5.3). §5.3.
    ForgetPreimage { target: ServiceId, hash: Hash, len: u32 },
    /// Deferred payout of the target's regular balance at wind-down. A
    /// drain-amount verb is an outstanding upstream ask: `TransferOut` fixes
    /// `amount` at emission, so an emission-sized payout races concurrent
    /// credits; `EjectReturn` sweeps the residue. Settlement needs the
    /// `PaidOut { id, amount }` echo, also outstanding. §5.3.
    PayoutRegular { target: ServiceId, dest: ServiceId },
    /// Sweeps both balances, amount state-determined at replay ⇒ settlement
    /// needs the `Ejected { id, swept_regular, swept_supervisor }` echo
    /// (an outstanding upstream ask) for the lease-return vs excess split (§6).
    EjectReturn { target: ServiceId },
}

// Lowering onto the Parachain Service API (design §3.3): `TransferOut` and `Reassign`
// lower to the supervision-aware `UpwardMessage::TransferOut` (foreign
// `source`, per-side supervisor-balance selectors, plain vs deferred).
// `CleanupStorage` lowers to `RemoveServiceStorage`, `ForgetPreimage` to
// `Forget { target: Target::Service, .. }`, `HandOverSupervision` to
// `SetServiceSupervisor`, and `EjectReturn` to `EjectService` — all landed; the
// `Ejected` id echo is outstanding upstream. `FreezeTarget` awaits
// `SetServiceCode`; `PayoutRegular` awaits a drain-amount verb.

enum OperationState {
    Requested,
    Submitted,
    Confirmed,
    Failed,             // JAM-side rejection observed; terminal —
                        // recovery is a new operation (§7.1)
    Burnt,              // governance-tier write-off (e.g. destination ejected
                        // before accumulation); the units stay locked forever (§6)
}
```

Aggregates maintained for reporting and the conservation check (§6):

```rust
struct Totals {
    reserve: Balance,          // attributed reserve = JAM reserve balance − excess,
                               // mirrored at last reconciliation anchor
    excess: Balance,           // inflows outside the cap identity (donations,
                               // stray sweeps, bad-memo returns, §5.3 payouts);
                               // entries with known provenance are reserved for
                               // their claimant (§6)
    in_flight_out: Balance,    // JAM-debited, not yet credited at destination (§6)
    released: Balance,         // outstanding permanent releases
                               // (net of attributed returns of released units)
    leased: Balance,           // active supervisor-balance allocations
    burnt: Balance,            // cumulative Burnt write-offs
    locked: Balance,           // Hub units in locked custody
                               // = in_flight_out + leased + released + burnt (§6)
}
```

Typed events: `AllocationApproved`, `OperationSubmitted`, `OperationConfirmed`,
`OperationFailed`, `OperationBurnt`, `ReturnCredited`, `Paused`, `Resumed`,
`Upgraded { from, to }`.

The contract ABI. Selector names are illustrative; the effect column is
normative. Every JAM-side effect is stated as the operation it creates (§3.2
`OperationKind`); selectors are entry points, operations are the effects.

| Selector | Origin | Effect |
| --- | --- | --- |
| `initialize(config)` | governance (Root) | binds the asset, mints `cap` into frozen custody (§3.1) |
| `attest_and_thaw(anchor)` | governance (Root) | records the genesis attestation, unfreezes (§3.1) |
| `approve_allocation(mode, target, amount, conditions)` | governance (Root) | records `Allocation{state: Approved}` |
| `set_operators(accounts)` | governance (Root) | registers the operator accounts |
| `pause()` / `resume()` | governance (Root) | freezes and thaws the asset via the Freezer role (§3.3) |
| `execute_allocation(id)` | operator | locks the units; creates `Operation{TransferOut}` per the allocation mode (§5.1, §5.2) |
| `freeze_target`, `cleanup_storage`, `forget_preimage`, `reassign`, `payout_regular`, `eject_return`, `hand_over_supervision` | operator | each creates the matching `Operation` kind (§5.3) |
| `redeem(amount, dest)` | any holder | locks the holder's units; creates `Operation{TransferOut, dest_supervisor: false}` (§5.2.2) |
| `settle(op_id)` | any signed account | applies the §2 verdict to the operation |

### 3.3 pallet-assets precompile

`pallet-assets` holds JAMKB; the manager contract needs a way to administer it.
Assets are already exposed to contracts as an ERC20 precompile, which carries no
administrative calls. This design extends it: a contract holding pallet-assets
roles performs the same administrative operations as a privileged signed
account.

The manager uses the selector set:

| Selector | pallet-assets call | Used for | Role required |
| --- | --- | --- | --- |
| `transfer` | `transfer` | moving units in/out of manager custody | (holder) |
| `approve` / `transferFrom` | `approve_transfer`, `transfer_approved` | transferring a holder's units into custody (`redeem`, §5.2.2) | (holder-authorized) |
| `mint` | `mint` | initialization only tokens mint | Issuer |
| `freeze` / `thaw` | `freeze`, `thaw` | emergency pause of the asset | Freezer |
| `set_team` / `transfer_ownership` | role admin | manager upgrade/migration only | Owner |

### 3.4 `pallet-jamkb` (JAMKB Operations on the Generic AH→JAM Transport)

`pallet-jamkb` is a FRAME pallet used to interact with the Parachain Service. It holds no
code that reaches the Parachain Service directly, so it is layered on a generic transport
mechanism. The planned [cumulus-on-jam](../cumulus-on-jam/cumulus-on-jam.md) §11
`validate_block` rework is expected to provide it, probably by extending `parachain-system`
into a generic AH→JAM transport pallet. The generic pallet would own the outbound upward
messages, their emission through `send_upward_message` inside `jam_validate_block`, and
the inherent that delivers the validation inputs.

#### The parachain-system pallet requirements

The design of this pallet is out of scope of this document. The requirements below are
what `pallet-jamkb` needs from it to operate.

The Parachain Service restricts `TransferOut` and its sibling upward messages: they are
accepted only from the Asset Hub parachain. Such `UpwardMessage` variants have no
public push API in the parachain-system pallet. The generic pallet pulls each restricted
variant class from one provider named in the runtime `Config`, following the
`XcmpMessageSource` pattern.

The generic pallet exposes the inherent data (log entries, incoming transfers) to pallets
as validation inputs.

#### What `pallet-jamkb` owns

The pallet exposes `request_operation(op)`, callable only by the manager contract.
It validates the operation, records the `OperationId`, appends it to `PendingOperations`
(the per-block emission queue), and sets the operation `Requested`. The parachain-system
pallet takes the queue through the source trait and calls `send_upward_message` for each
message; the runtime `Config` names `pallet-jamkb` as the only `TransferOut` provider.

The manager contract reaches `pallet-jamkb` through a precompile. For authorization,
`pallet-jamkb` keeps the manager contract's `AccountId` and
rejects any other caller.

The generic pallet's inherent verifies `(anchor, proof, para head, log entries, incoming
transfers)` against the anchor's posterior state-root. `pallet-jamkb` stores what the
inherent verified, exposes it through the precompile, and the manager contract
consumes it via `settle(op_id)`.

---

## 5. Allocation Protocols

### 5.1 Lease (Supervisor-Managed Allocation)

A lease is a tokens transfer to the target service's supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor.

```
Phase 1: Approve      Governance approves Allocation{mode: Lease, target, amount}.
Phase 2: Lock         Manager locks `amount` JAMKB and calls `request_operation`
                      with Operation{TransferOut, dest_supervisor: true} (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a plain `transfer` (reserve → target's supervisor
                      balance; deferred = None).
Phase 4: Confirm      Any party can call `settle(op_id)` on the manager (§2).
                      The output:
                      Confirmed: the credit sits on the target's supervisor
                      balance; the locked units stay locked, backing the
                      lease (§6).
                      Failed: JAM rejected the transfer; the units unlock (§6).
```

### 5.2 Permanent Release

A permanent release is a tokens transfer to the target service's regular balance.

#### 5.2.1 Governance-initiated Release

Governance releases units from DAO custody

```
Phase 1: Approve      Governance approves Allocation{mode: Permanent, target, amount}.
Phase 2: Lock         Manager locks `amount` JAMKB and calls `request_operation`
                      with Operation{TransferOut, dest_supervisor: false} (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a deferred `transfer` (reserve → target's regular
                      balance; memo §7.2).
Phase 4: Confirm      Any party can call `settle(op_id)` on the manager (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units stay locked.
                      Failed: JAM rejected the transfer; the units unlock.
```

#### 5.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service, bypassing
governance:

```
Phase 1: Approve      Holder calls `approve(manager, amount)` on the JAMKB
                      asset (§3.3): the manager may transfer up to `amount` of
                      the holder's units.
Phase 2: Lock         Holder calls `redeem(amount, target)`. Manager locks the
                      units (a `transferFrom` into custody, §3.1) and calls
                      `request_operation` with
                      Operation{TransferOut, dest_supervisor: false} (§3.4).
Phase 3: Submit       pallet-parachain-system emits the TransferOut via
                      `send_upward_message`; the Parachain Service executes it
                      as a deferred `transfer` (reserve → the target's regular
                      balance; memo §7.2).
Phase 4: Confirm      Any party can call `settle(op_id)` on the manager (§2).
                      The output:
                      Confirmed: the credit sits on the target's regular
                      balance, outside DAO control; the units stay locked.
                      Failed: JAM rejected the transfer; the units unlock, back
                      to the holder account.
```

### 5.3 Lease Return

Full return, cooperative (the standard end of a lease).

```
Phase 1: Shrink       Target deletes its own state until its residual footprint
                      is covered by its own balance.
Phase 2: Reassign     Manager calls `request_operation` with
                      Operation{Reassign{target, amount}}, amount = the full
                      lease (§3.4).
Phase 3: Submit       pallet-parachain-system emits the lowered TransferOut
                      (§3.2) via `send_upward_message`; the Parachain Service
                      executes it as a plain `transfer` (target's supervisor
                      balance → reserve; deferred = None). It fails if the service
                      balance + supervisor_balance < threshold balance.
Phase 4: Confirm      Any party can call `settle(op_id)` on the manager (§2).
                      The output:
                      Confirmed: the units unlock (§6); the target is handed
                      back to self-supervision
                      (HandOverSupervision(target, target)).
                      Failed: JAM rejected the transfer; the lease stays Active.
```

Full return, non-cooperative. Entered when the lease has ended
and the target has not freed the footprint and the flow above failed.

Supervision gives the manager full power over the target, including cleaning
its state and ejecting it. Exercising that power breaks the expectation that
services are unstoppable. An alternative could avoid it: collateral is set at
lease allocation, and an unreturned lease is treated as a sale. The collateral
is charged and the target keeps running. The drawback is that funds equal to
the token sale price need to be locked for the lease duration.

On the other side, if the forced-cleanup direction is taken to release the
tokens, the gap of the lacking storage keys remains (see the storage-keys
note below). The flow with enforced cleanup:

```
Phase 1: Freeze       Manager submits `FreezeTarget`. Parachain Service issues
                      a foreign `SetCode` to a 32-byte preimage-free hash
                      (e.g. zero).
Phase 2: Drain        Confirm all outbound transfers to the target have
                      settled. The manager rejects new operations.
Phase 3: Cleanup      Manager submits `CleanupStorage` pages; the Parachain
                      Service executes `RemoveServiceStorage { target, key }`
                      for each key. Manager submits `ForgetPreimage(hash, len)`
                      for every preimage except the target's code preimage.
                      The exit fork:
                      (a) RESTORE: once the target's own balance covers its
                          reduced footprint, the leased balance returns via
                          the cooperative flow above; unfreeze via `SetCode`
                          restoring the original code hash;
                          HandOverSupervision(target, target) ends the
                          wind-down, leaving the target self-supervised.
                      (b) TERMINATE: continue below.
Phase 3b: Forget      Discard the code preimage: `ForgetPreimage` now and
                      again after `C_expungeperiod` (~32 h); eject fails
                      `NotEmpty` until the preimage is expunged.
Phase 4: Payout       Manager submits `PayoutRegular`; the Parachain Service
                      defers the target's regular balance back to the Parachain
                      Service itself. The amount books as excess with its
                      provenance: owner-claimable (§6).
Phase 5: Confirm      The payout settles by a `settle(op_id)` call on the
                      manager. The eject may be submitted only after this
                      settles Confirmed.
Phase 6: Eject        Manager submits `EjectReturn`; `eject(target)` sweeps
                      the remaining balances to the Parachain Service. The
                      swept supervisor balance counts as the lease return up
                      to `leased`; any surplus (third-party credits, payout
                      dust) books as excess (§6).
Phase 7: Confirm      The eject settles the same way; the lease-return units
                      unlock (§6).
```

- A frozen target cannot solicit or write. Freeze prevents
  the target from pinning more footprint.
  A frozen self-supervised service is permanently unrecoverable: it can
  neither be unfrozen nor ejected, as both require a supervisor other than
  the target itself.

- Storage keys are not recoverable from state and must be tracked
  externally. Without keys, cleanup is impossible.

### 5.4 Voluntary Return

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service reserve.

```
Phase 1: Submit       A service sends a deferred transfer to the Parachain
                      Service (memo = return attribution, §7.2); the Parachain
                      Service queues it in `incoming_transfers`.
Phase 2: Confirm      The manager records the return from the stored
                      validation inputs (§3.4); any party can trigger it.
                      The output:
                      Valid memo (§7.2): the named Asset Hub account is
                      credited from locked custody.
                      Missing or malformed memo: funds are classified as excess (§6),
                      recorded with its source; a refund to the source requires an
                      operator action.
```

- The Parachain Service cannot refuse an incoming transfer. JAM credits the
  destination before its code runs. Its only decision is whether the transfer
  is recorded. The queue's reserved portion (`MAX_INCOMING_TRANSFERS`) records
  unconditionally. Beyond it the queue is self-funding: an entry is recorded
  only if the transferred `amount` covers its own queue-slot cost. Below that
  floor the funds are kept but the transfer goes unrecorded. Without a
  governance action they stay unusable.

---

## 6. Cap & Backing Accounting

Conservation:

```
cap  =  unlocked + locked                                                (Hub view)
     =  reserve + in_flight_out + leased + released + burnt              (JAM view)

unlocked       =  user balances + undistributed custody (§3.1)  =  reserve
locked         =  in_flight_out + leased + released + burnt

where
  reserve        =  the Parachain Service balance above the segregation floor
                    (§3.2; the floor covers the platform's own footprint),
                    minus excess
  in_flight_out  =  deferred transfers where the source
                    has been charged but the target not yet credited; the funds
                    sit in no service balance.
  released       =  outstanding releases, net of matched returns (§5.4)
  excess         =  the unattributed slice of the Parachain Service balance
                    (bad-memo returns, donations, reclaim surplus above
                    `leased`), counted by the manager field `Totals.excess`
                    (§3.2); an unrecorded return enters the count only when
                    governance books it. Outside the identity, disposed
                    by governance. An entry with known provenance (a payout, a
                    sourced return) is reserved for its claimant and is never
                    converted into backing
```

---

## 7. Message Protocol

### 7.1 Operations, Correlation

- An `OperationId` is unique and never reused. The transfer sent to JAM carries the
  `OperationId` as its `id` field. On failure, `TransferFailed { id }` returns
  the same id, so the entry points directly at the failed operation.

### 7.2 Memo Requirements

JAM transfer memos are 128 octets. A voluntary return carries the
beneficiary account in it. The exact layout is to be defined.

---

## 8. References

- [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926): Burn of all DAO proceeds from JAMKB; no grants, gifts, or below-market loans
- [JAM Gray Paper](https://graypaper.com): Formal JAM specification (Gavin Wood)
- [Parachain Service on JAM](../parachain-service-on-jam/parachain-service-on-jam.md)
- [DOT DAO and the need for $JAMKB](https://medium.com/polkadot-network/dot-dao-and-the-need-for-jamkb-a069e72e9728): Gavin Wood
- [DOT DAOism under JAM: An Island Story](https://medium.com/polkadot-network/dot-daoism-under-jam-an-island-story-efe0d02ee084): Gavin Wood
