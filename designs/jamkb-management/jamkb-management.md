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
   - 5.3 [Reassignment and Full Return](#53-reassignment-and-full-return)
   - 5.4 [Voluntary Return (Inbound Leg)](#54-voluntary-return-inbound-leg)
6. [Cap & Backing Accounting](#6-cap--backing-accounting)
7. [Message Protocol](#7-message-protocol)
   - 7.1 [Operations, Correlation, Idempotency](#71-operations-correlation-idempotency)
   - 7.2 [Memo Encoding](#72-memo-encoding)
   - 7.3 [Confirmation & Finality](#73-confirmation--finality)
   - 7.4 [Failure Taxonomy](#74-failure-taxonomy)
   - 7.5 [Reconciliation](#75-reconciliation)
8. [Open Items & Dependencies](#8-open-items--dependencies)
   - 8.1 [Project definition (owned by the DOT DAO)](#81-project-definition-owned-by-the-dot-dao)
   - 8.2 [Upstream scope changes (Parachain Service and platform)](#82-upstream-scope-changes-parachain-service-and-platform)
   - 8.3 [Internal technical decisions (owned by this design)](#83-internal-technical-decisions-owned-by-this-design)
9. [References](#9-references)

---

## 1. Overview

This document describes the architecture of JAMKB management on Asset Hub.
JAMKB is JAM's resource-access token for state footprint. A JAM service may keep
as much state as its balance covers. Asset Hub carries a 1:1 representation of
the token, where it is managed, sold and leased.

### Scope

This document covers:

- The flows: lease, permanent release, reassignment and return
- The components they use: the JAMKB asset, the manager contract, and the
  administration precompile
- The command path through the Parachain Service

This document does not cover economic policy: how JAMKB is priced, sold or distributed.

---

## 2. Architecture Overview

Initially all JAMKB sits on the Parachain Service; this document calls that
balance the reserve. Asset Hub holds its 1:1 representation (§3.1). Both
levels track the same cap:

```
Level 2 — Asset Hub (derived)
  pallet-assets JAMKB:
    user balances      — spendable units against the reserve
    manager custody    — undistributed and locked units

Level 1 — JAM balances:
    reserve                          — a Parachain Service balance;
                                       backs everything spendable on the Hub
    recipients' supervisor balances  — leases (DAO-controlled, recoverable)
    recipients' regular balances     — permanent releases (outside DAO control)
```

The flow below is a governance-executed permanent release (§5.2):
one deferred transfer from the reserve to the target JAM service's regular
balance. A lease follows the same path, crediting the supervisor balance instead.

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
   │  settle(op_id), callable by any party (§7.3) provides the transfer state:
   │  - para head at or past B, no TransferFailed for the attempt in the stored
   │  inputs → Confirmed
   │  - a TransferFailed for the attempt → Failed, unlock per §6
   │  - head still below B → stays pending;
```

---

## 3. Asset Hub Components

### 3.1 The JAMKB Asset

JAMKB is an asset in `pallet-assets`. It is the representation of the DAO's
balance on the Parachain Service. This asset is managed by manager contract.
It holds the four privileged roles (Owner, Issuer, Admin,
Freezer), assigned to it at initialization.

The full JAMKB cap is minted into the manager contract's account. The mint is a one-time governance-executed runtime call on the Asset Hub.

When a transfer to a JAM service is executed, the transferred amount is locked
on the Asset Hub side. Tokens are unlocked only upon a confirmed return or a
transfer failure (§6).

At bootstrap, JAM services need a service balance to operate, like the Parachain Service
itself. The amount W assigned to a JAM service at genesis is therefore locked on the
Asset Hub side after the mint and recorded as released₀

### 3.2 Manager Contract

A pallet-revive contract holding the asset roles and the operation records.
It executes and monitors all transfer

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
    conditions: BoundedVec<u8, MAX_CONDITIONS>,  // opaque governance terms
}

enum AllocationMode { Lease, Permanent }

enum AllocationState {
    Approved,
    Delivering(OperationId),          // lease credit in flight
    Active,                           // lease live
    Releasing(OperationId),           // atomic convert+hand-over in flight (§5.2)
    Released,                         // hand-over event confirmed; terminal for control
    Reclaiming(OperationId),          // reassignment / wind-down in progress
    Closed,                           // returned, converted, or written off
}

/// One cross-system operation (a single JAM-side effect).
struct Operation {
    id: OperationId,                  // unique correlation id, §7.1
    attempt: u16,                     // retry nonce; memo carries (id ‖ attempt), §7.1
    kind: OperationKind,
    allocation: Option<AllocationId>,
    amount: Balance,
    state: OperationState,
    submitted_at: BlockNumber,
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
    /// Atomic in one Parachain Service accumulate invocation:
    /// immediate sup→reg conversion and supervisor(target, target). §5.2.
    ConvertAndRelease { target: ServiceId },
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
    /// unprovided solicits need execution capture (§5.3 key-knowledge note).
    /// The target's own code preimage is forgotten last, and only on the
    /// terminate exit (§5.3). §5.3.
    ForgetPreimage { target: ServiceId, hash: Hash, len: u32 },
    /// Deferred payout of the target's regular balance at wind-down. A
    /// drain-amount verb is outstanding (§8.2): `TransferOut` fixes `amount`
    /// at emission, so an emission-sized payout races concurrent credits;
    /// `EjectReturn` sweeps the residue. Settlement needs the
    /// `PaidOut { id, amount }` echo (§8.2). §5.3.
    PayoutRegular { target: ServiceId, dest: ServiceId },
    /// Sweeps both balances, amount state-determined at replay ⇒ settlement
    /// needs the `Ejected { id, swept_regular, swept_supervisor }` echo
    /// (§8.2) for the lease-return vs excess split (§6).
    EjectReturn { target: ServiceId },
    CreditReturn { beneficiary: AccountId, amount: Balance },   // inbound
}

// Lowering onto the Parachain Service API (design §3.3): `TransferOut` and `Reassign`
// lower to the supervision-aware `UpwardMessage::TransferOut` (foreign
// `source`, per-side supervisor-balance selectors, plain vs deferred).
// `CleanupStorage` lowers to `RemoveServiceStorage`, `ForgetPreimage` to
// `Forget { target: Target::Service, .. }`, `HandOverSupervision` to
// `SetServiceSupervisor`, and `EjectReturn` to `EjectService` — all landed; the
// `Ejected` id echo is outstanding (§8.2). `FreezeTarget` awaits `SetServiceCode`;
// `ConvertAndRelease` is `TransferOut` (sup→reg, self) plus
// `SetServiceSupervisor(target, target)` in one digest, atomicity note outstanding;
// `PayoutRegular` awaits a drain-amount verb (§8.2).

enum OperationState {
    Requested,
    Submitted,
    Confirmed,
    Failed,             // JAM-side rejection observed (retryable)
    Burnt,              // destination ejected before accumulation; funds destroyed
    RecoveryRequired,   // outcome unobservable; manual reconciliation
}
```

Aggregates maintained for reporting and the conservation check (§6):

```rust
struct Totals {
    reserve: Balance,          // attributed reserve = JAM reserve balance − excess,
                               // mirrored at last reconciliation anchor
    excess: Balance,           // unattributed inflows (donations, stray sweeps) —
                               // outside the cap identity, disposed by governance
    in_flight_out: Balance,    // JAM-debited, not yet confirmed at destination
    released: Balance,         // outstanding permanent releases
                               // (net of attributed returns of released units)
    leased: Balance,           // active supervisor-balance allocations
    burnt: Balance,            // cumulative Burnt write-offs
    locked: Balance,           // Hub units locked pending confirmation
    in_flight: u32,            // count of non-terminal operations
}
```

Typed events: `AllocationApproved`, `OperationSubmitted`, `OperationConfirmed`,
`OperationFailed`, `OperationBurnt`, `LeaseConverted`,
`ReturnCredited`, `Paused`, `Resumed`, `Upgraded { from, to }`.

The error model, ABI encoding and full selector list follow pallet-revive conventions
and are not specified here.

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
inherent verified and exposes it through the precompile and the manager contract conumes it by `settle(op_id)`.

---

## 5. Allocation Protocols


### 5.1 Lease (Supervisor-Managed Allocation)

A lease is a single plain-move transfer from the reserve to the target service's
supervisor balance.
Precondition: the Parachain Service is the target's effective supervisor.

```
Phase 1: Approve      Governance approves Allocation{mode: Lease, target, amount}.
Phase 2: Lock         Manager locks `amount` JAMKB tokens; Request Operation{TransferOut,
                      dest_supervisor: true}.
Phase 3: Submit       Upward message → Parachain Service accumulate → plain `transfer`
                      (dest = target, credit supervisor balance, deferred = None).
Phase 4: Confirm      Settles via §7.3. On success, Allocation → Active.
                      On a `TransferFailed` validation input: Failed (unlock per §6).
```

### 5.2 Permanent Release

A permanent release is a single deferred transfer from the reserve to the target
service's regular balance.

#### 5.2.1 Governance-initiated Release

Governance releases units from DAO custody

```
Phase 1: Approve      Governance approves Allocation{mode: Permanent, target, amount}.
Phase 2: Lock         Manager locks `amount`; Operation{TransferOut,
                      dest_supervisor: false} → Requested.
Phase 3: Submit       Deferred transfer to the target's regular balance, memo = op id.
Phase 4: Confirm      Settles via §7.3. Allocation → Released, `released += amount`.
                      On a `TransferFailed` validation input: Failed (unlock per §6).
```

#### 5.2.2 Holder-initiated Release

Any holder of spendable units may release their own units to a JAM service, bypassing
governance:

```
Phase 1: Lock         Holder calls `redeem(amount, dest)`. Manager locks the units.
                      Operation{TransferOut, dest_supervisor: false} → Requested.
Phase 2: Submit       Deferred transfer to `dest`'s regular balance, memo = op id.
Phase 3: Confirm      Settles via §7.3. Units stay locked, `released += amount`.
                      On Failed: unlock per §6, back to the holder account.
```

### 5.3 Lease Return

Full return, cooperative (the standard end of a lease). The target frees the
footprint itself; no enforcement steps needed:

```
Phase 1: Shrink       Target deletes its own state until its residual footprint is
                      covered by its own balance.
Phase 2: Reassign     Manager submits Operation{Reassign{target, amount}} 
                      amount = the full lease.
Phase 3: Submit       Parachain Service executes immediate transfer
                      (target supervisor balance → reserve).
                      Fails if balance + supervisor_balance < threshold balance.
Phase 3: Confirm      Settles via §7.3. If success Hub units unlocked.
                      A target is handed back to
                      self-supervision (`HandOverSupervision(target, target)`).
```

Full return, non-cooperative. Entered when the lease has ended
and the target has not freed the footprint and the flow above failed.
The flow stops at any phase where the
target starts cooperating then it completes as above:

```
Phase 1: Freeze       Manager submits `FreezeTarget`. Parachain Service issues
                      foreign `SetCode` to a 32-byte preimage-free hash (e.g. zero).
Phase 2: Drain        Confirm all outbound transfers to target have settled.
                      The manager rejects new operations naming a target whose
Phase 3: Cleanup      Manager submits `CleanupStorage` pages.
                      Parachain Service execute for each key `RemoveServiceStorage { target, key }`.
                      Manager submits `ForgetPreimage(hash, len)` for all preimages
                      except the target's code preimage.
                      Exit fork decides Phase 4 Path:
                      (a) RESTORE: Owner tops up target's regular balance. Return the
                          remaining supervisor balance to the reserve. Unfreeze via `SetCode`
                          restoring original codehash. `HandOverSupervision(target)`.
                      (b) TERMINATE: Discard the code preimage. Payout remaining balances.
Phase 3b: Forget      Second `ForgetPreimage` round after `C_expungeperiod`
          again        (~32 h), driven by `ForgetAgainAt { due }` entries;
                      preimage requests block eject until expunged.
Phase 4: Payout       In TERMINATE path: Parachain Service defers transfer of target's
                      regular balance to a governance destination.
Phase 5: Confirm      Settles via §7.3.
Phase 6: Eject        Manager submits `EjectReturn`. Parachain Service executes `eject(target)`
                      sweeping remaining balances as the lease return to the Parachain Service.
Phase 7: Confirm      Settles via §7.3. Hub units unlocked.
```

**Atomic release.** The escrowed variant's `ConvertAndRelease` (§5.2) transfers first, then hands over
supervision. Both ride one accumulate invocation; all-or-nothing depends on the
§8.2 checkpoint note, outstanding upstream.

**Freeze.** A frozen target cannot solicit, write, or re-supervise. Freeze prevents
the target from pinning footprint during reassignment.

**Sovereign freeze.** A frozen self-supervised service is permanently unrecoverable.

**Storage keys.** Keys are not recoverable from state and must be tracked
externally. Without keys, cleanup fails and residue is stranded.

### 5.4 Voluntary Return (Inbound Leg)

Any service may return regular JAMKB by a deferred transfer to the Parachain
Service reserve. The transfer carries an attribution memo (§7.2). The flow is
inbound: the returning service initiates it, so no Hub operation exists before
the transfer is observed.

```
Phase 1: Send         A service sends a deferred transfer to the reserve,
                      memo = attribution (§7.2).
Phase 2: Record       The Parachain Service queues the transfer in
                      `incoming_transfers`. Below the self-funding floor the
                      transfer is kept but unrecorded (first note below).
Phase 3: Observe      Asset Hub reads the entry through the validation inputs
                      at the bound anchor (§3.4). The manager records the
                      return.
Phase 4: Credit       Attributable: the named Asset Hub account is credited,
                      exactly once, after confirmation; `released -= amount`
                      when attributable to a release. Unattributable: refund
                      to the source service, else excess (§6).
Phase 5: Consume      Asset Hub emits `CleanUpBucketsUpTo(bucket_id)` for
                      buckets it has read. Nothing unread is removed
                      (second note below).
```

- The Parachain Service **cannot refuse an incoming transfer**: JAM credits the
  destination before its code runs, and there is no bounce. Its only decision is
  whether the transfer is *recorded*. The queue's reserved portion
  (`MAX_INCOMING_TRANSFERS`) records unconditionally. Beyond it the queue is
  **self-funding**: an entry is recorded only if the transferred `amount` covers
  its own queue-slot cost. Below that floor the funds are kept but the transfer
  goes **unrecorded**: never observed, never attributed, surfacing only as
  `excess` at reconciliation. The minimum-return amount is therefore an
  economic floor to publish, not an admission rule enforced by refusal. Beyond
  the reserved portion each recorded entry pays for itself. Inside it, recording
  is unconditional: dust can occupy the pre-provisioned slots (SPEC_GAPS #2), at
  most `MAX_INCOMING_TRANSFERS` entries of attribution delay.
- **Exactly-once is defined by queue position.** `incoming_transfers` is held in
  fixed-size buckets under contiguous ids, and `CleanUpBucketsUpTo(bucket_id)` removes
  every bucket up to and including `bucket_id`. Asset Hub names only bucket ids it has
  read through the validation inputs, and the JAM block it references only advances, so
  nothing unread is ever removed (Parachain Service design §5.1). Identity is never
  taken from the memo. Two entries with identical memos are two distinct returns; a
  front-runner replaying a victim's memo cannot swallow the victim's entry. Asset
  Hub reorg safety follows from the same rule: consumption emissions
  accumulate only on the parent-head-checked canonical Asset Hub chain, so a reorg
  cannot double-consume.
- Attributable returns credit the specified Asset Hub account, exactly once,
  after confirmation.
- Unattributable returns (malformed memo): default **refund by an explicit
  transfer back to the source service**; if that is inadvisable (e.g. source
  gone), funds accrue to the **excess** bucket (§6) and are recorded
  `ReturnCredited{beneficiary: none}` pending governance disposition.
- A confirmed return of previously **released** units decrements outstanding
  `released` when attributable to a release (else lands in excess). This keeps
  the §6 identity closed under returns.
- Whether voluntary return is permissionless is undecided (§8.1). If yes,
  circulating receipt supply becomes market-elastic: units re-enter Hub supply on
  anyone's return. The decision (§8.1) names that monetary consequence.

---

## 6. Cap & Backing Accounting

Conservation:

```
cap  =  spendable_hub + locked_hub                                       (Hub view)
     =  reserve + in_flight_out + leased + released + burnt              (JAM view)

spendable_hub  =  reserve                (1:1 backing of every spendable unit)
locked_hub     =  in_flight_out + leased + released + burnt

where
  reserve        =  attributed reserve = the Parachain Service balance above the
                    segregation floor − excess
  in_flight_out  =  amounts debited on JAM but not yet settled on the Hub. For a
                    deferred transfer (releases, redemption) the funds are in
                    delivery limbo and belong to no service balance; for a plain
                    move (leases) the credit is immediate and only the Hub-side
                    observation is outstanding
  released       =  outstanding releases, net of attributed returns
  excess         =  unattributed inflows (donations, stray sweeps): outside the
                    identity, reported separately, disposed by governance
```

The Hub-side terms are maintained at every manager state transition; the JAM-side
reads that *check* the identity are anchored at reconciliation points (§7.5). Between
anchors, one-round delivery skew (§7.3) is expected and is not a discrepancy.
The **JAM records are canonical**; the Hub representation is derived. On discrepancy,
JAM prevails and the Hub is repaired toward it, never the reverse.

Lock/unlock rules per transition:

| Transition | Hub units | JAM funds |
| --- | --- | --- |
| Allocation approved | — | — |
| Operation submitted | locked (`in_flight_out += amount`) | debit on `transfer` OK |
| Confirmed (outbound) | remain locked (`in_flight_out −= amount`; `leased += amount` for a lease, `released += amount` for a release) | credited at destination |
| Confirmed (return) | unlocked exactly once (`released −= amount` when attributable to a release, §5.4) | credited to reserve |
| Failed | unlocked only when every emitted attempt has a settled failure validation input (`in_flight_out −= amount`) | never debited (host-call rejected) |
| Unrecorded return | n/a (no locked Hub units) | kept by the Parachain Service unobserved (surfaces as `excess`) |
| RecoveryRequired | remain locked (exit via reconciliation §7.5) | unknown until settled |
| Cancelled (`Requested`) | unlocked (op closed) | never debited |
| **Burnt** | written off (stay locked forever, `burnt += amount`, `in_flight_out −= amount`) | destroyed |

There is no bounce. JAM credits an incoming transfer before the destination's code runs. The
Parachain Service cannot refuse a transfer. It only chooses whether the transfer is recorded in
`incoming_transfers`.

| Flow | Destination | Burn reachable? |
| --- | --- | --- |
| Lease outbound | recipient supervised by the Parachain Service | No |
| Release (escrowed variant) | recipient supervised by the Parachain Service during delivery | No |
| Release (default) | recipient's own supervision state | Only by recipient itself |
| Full-return payout | governance-designated service | Guarded by notice/drain discipline |
| Reassignment / returns | Parachain Service / reserve | No |
| Third-party ↔ third-party | anyone | Yes (outside manager books) |

**Failure and burn notes:**
- A failure unlock requires a per-attempt `TransferFailed` (§7.3/§7.1). A rejected call entails no
  debit. A retry re-locks first.
- A burn requires the destination of an in-flight deferred transfer to be ejected. Only a supervisor
  can eject a service.
- In manager-mediated flows, a third party can never trigger a burn.
- `Burnt` is never signal-driven. A burn appears in no log (dropped in Ψ_A).
- The only path into `Burnt` is the §7.5 governance tier.
- Guard violations make burning observable. A burn loses value but never mints an unbacked spendable
  unit.

**Stranding (distinct from `Burnt`).** A frozen, uncooperative target with
uncaptured keys pins the slice of its lease that collateralizes its stored data.
The funds exist on JAM as the deposit of an unremovable service: `items > 0`
blocks eject, and the protocol has no eviction. No party can reach them. Reporting
splits `leased` into *recoverable* and *stranded*; the conservation
identity is unchanged (stranded ⊂ leased; Hub units stay locked). Exit paths:
cooperative shrink, later key capture (§5.3), GP-level purge (§8.2). Bounded
ex ante by the §8.1 protection policy; under collateral, a strand reclassifies
as a completed sale.

---

## 7. Message Protocol

**This protocol is not XCM.** All outbound commands ride the
Parachain Service's *side-effect channel*: typed `UpwardMessage` variants recorded
in the work digest at Refine and replayed as JAM host-calls by `accumulate`
(Parachain Service design §3.3, §4.3). Despite the name, these are **not** UMP/XCM
messages: no XCM encoding, router, or executor is involved anywhere, and JAM
services do not interpret XCM. The XCMP/UMP/HRMP messaging layer proper is
unspecified on JAM (Parachain Service design §8.2; cumulus-on-jam phase 1 ships
without messaging) and this design takes **no dependency on it**. JAMKB operations work in
a no-messaging phase-1 world.

### 7.1 Operations, Correlation, Idempotency

- `OperationId`: a unique correlation identifier, never reused for the life of the
  system, including after its operation record is pruned. The allocation state
  survives manager upgrades. Allocation is an implementation detail; standard
  pallet counters suffice.
- **Attempts are first-class**: a retry reuses the `OperationId` with an
  incremented `attempt` nonce, and the memo carries `(op_id ‖ attempt)`. Every
  emission is individually attributable; an acknowledgement binds to a
  *specific attempt*, never the operation in the abstract. Settling against a
  validation input of a different attempt is rejected. This makes the §6 `Failed`
  unlock rule checkable: "no attempt credited and no attempt still able to
  accumulate" quantifies over attempt nonces.
- Exactly-once: the manager rejects acknowledgements for unknown or terminal
  operations; duplicate confirmations are no-ops.
- **Retry gate**: a new attempt may be emitted only when every prior attempt of
  the same `OperationId` is JAM-terminal by validation input (a settled per-attempt
  failure), never on operator belief. The gate closes the residual double-delivery risk: an earlier
  deferred attempt
  still in flight when the retry lands. Double delivery never violates the 1:1
  backing, since each attempt re-locks first. It over-delivers to the recipient.
- Delayed messages are interpreted against the operation's current state, never
  applied blindly.
- Failure logs echo the caller-chosen transfer id (`TransferFailed { id, error }`
  with typed `TransferError` reasons): a fresh `id: u64` is assigned per emission
  (per attempt), never reused, and indexed to `(OperationId, attempt)`. It has the
  same uniqueness and upgrade-survival properties as the `OperationId`. Memo-hash
  indexing is no longer needed for outbound failures.
- Retention: terminal operation records and the memo-hash index are kept for at
  least the confirmation window plus the reconciliation horizon, then prunable
  under governance policy; pruning never recycles ids. The validation-input
  store follows the same horizon: an entry may serve as another operation's
  eviction witness (§7.3), so it is never pruned sooner.

### 7.2 Memo Encoding

JAM transfer memos are exactly **128 octets**. `version != 0xFF` structurally
excludes the all-ones memo, which an earlier Parachain Service design reserved.

```rust
/// 128-octet memo layout. `version != 0xFF` structurally excludes [0xFF;128].
struct Memo {
    version: u8,          // 0x01
    kind: u8,             // 0x01 outbound-op, 0x02 return-attribution, ...
    op_id: [u8; 16],      // outbound: manager OperationId; return: zeroed
    attempt: u16,         // outbound: retry nonce (§7.1); return: sender nonce
    body: [u8; 102],      // kind-specific, zero-padded:
                          //   return-attribution: AccountId32 ‖ amount u128 LE ‖ reserved
    checksum: [u8; 6],    // truncated blake2 of bytes 0..122
}
```

The sender nonce and amount in the return body keep repeated returns by the same
sender distinguishable for attribution and reconciliation; attribution itself is by
queue position, never memo identity (§5.4). (Outbound failures are keyed by the
`TransferOut` caller id, not the memo (§7.1).)

The `kind` registry and the return-attribution body (account format, optional
beneficiary types) are not final (§8.3).

### 7.3 Confirmation & Finality

Asset Hub processes confirmations by checking the `parachain_log` using validation inputs tied to
the §3.4 anchor rule. An operation settles based on three verified conditions:

1. **Replay committed**: The para head from the lookup-anchor must match the head of the emitting
   Asset Hub block `B` or its descendant. This proves `B`'s messages were executed: the
   Parachain Service writes the head and replays a block's messages in one step
   (Parachain Service design §5.1 steps 6 and 7). The atomicity is the §8.2
   head-write-commits-replay note, still advisory upstream.
2. **No failure recorded**: No `TransferFailed { id, .. }` exists for the attempt's transfer
   `id` (§7.1) in the union of the cumulative validation-input store and Asset Hub's
   `parachain_log` snapshot. Earlier attempts' settled failures do not block a later attempt.
3. **Eviction witness**: A rank-2 entry older than `B`'s accumulation timeslot must exist in the
   snapshot to prove `B`'s failure entry was not evicted. If no witness exists, success is permitted
   only if the pallet's submission accounting proves rank-2 bytes are below the cap headroom.
   Otherwise, status becomes `RecoveryRequired` (§7.4).

Asset Hub ingests confirmations via a mandatory inherent (the JAM equivalent of
`parachain-system::set_validation_data`).
- The collator supplies `(anchor, proof, para head, log entries, incoming transfers)`.
- The generic pallet's inherent verifies the proof and records the validation inputs (§3.4).
- Operation state advances by pull: any party may call `settle(op_id)`, which reads the
  pre-verified validation inputs and updates the status. The caller pays the call's fee.
  `settle` is counterparty-neutral: it can only move the operation to the verdict the
  inputs dictate. Fees are never deducted from custody.

**Absence of failure.** The absence of a failure entry is sound solely because of the anchor
binding rule, which keeps the observed and pruned states in parity.

**Missing specification.** The exact validation-inputs/state-proof format remains undefined
(SPEC_GAPS #1).

### 7.4 Failure Taxonomy

| Signal | Meaning | Terminal? | Hub action |
| --- | --- | --- | --- |
| host-call rejection (`WHO/HUH/CASH/LOW`) | nothing happened (no debit) | no | retry / reconcile → `Failed` |
| `TransferFailed` log | accumulate-level rejection | no | reconcile → `Failed` |
| inbound return unrecorded (below queue-entry cost) | funds credited but never queued/observed | no | invisible until reconciliation; classified `excess` |
| destination ejected pre-accumulation | funds dropped, no refund; **appears in no log** — derivable only by reconciliation (debit established per §7.3, credit absent, destination gone per public JAM state) | **yes** | `Burnt`, write-off, alarm (governance-tier transition, §7.5) |
| no signal within window | outcome unknown | no | `RecoveryRequired` |
| submitted op's entry absent from a complete snapshot, never ingested (eviction window, §7.3) | outcome evidence destroyed | no | `RecoveryRequired` — never success |

### 7.5 Reconciliation

Reconciliation repairs the derived records toward JAM and is the **only** path
that unlocks units out-of-band. It can move value between the §6 buckets, so it
is privileged attack surface with its own authorization model.

Reconciliation has two tiers, split by frequency and by what can be proven
in-runtime:

- **Automated tier (frequent)** settles operations from the §7.3 machinery alone: the
  proven head, the failure log, the incoming-transfer queue. All
  inputs are Parachain Service state carried by the mandatory inherent; no
  account reads are needed.
- **Governance tier (rare)** covers the account-leaf-dependent repairs. JAM account
  state (the Parachain Service balance, a target's existence and creation timeslot)
  is public and checkable by anyone on any node, but not provable in-runtime:
  the validation inputs carry Parachain Service state only. These repairs are
  therefore **governance acts on public JAM-state evidence**, executed by Root:
  the genesis attestation and thaw (§3.1),
  re-anchoring `Totals.reserve`, and writing off `Burnt` (the "debit executed"
  leg is established by §7.3; the "destination gone" leg is the public-state
  evidence).
- **Repairs it may perform**: settle `RecoveryRequired` to `Confirmed`/`Failed`
  (automated tier, on validation inputs); unlock a `Failed` op's units (only when
  every emitted attempt has a settled per-attempt failure validation input, §6/§7.1);
  classify inflows into `excess`; and, governance tier: attest, re-anchor,
  write off.
- **Repairs it may never perform**: mint, raise the cap, unlock without settled
  per-attempt validation inputs, reclassify an allocation's mode, or bypass
  exactly-once.
- Every repair emits a typed event recording the anchor (automated tier) or the
  cited public-state evidence (governance tier). Operator assertions are hints,
  never inputs.

---

## 8. Open Items & Dependencies

Split by who owns the answer: the DOT DAO (8.1), the Parachain Service &
platform teams (8.2), or this design (8.3).

### 8.1 Project definition (owned by the DOT DAO)

The first rows are **foundational clarifications**: they gate the genesis
artifact and the economic viability of JAMKB itself, though not the mechanism,
since §3.1 keeps the design invariant to them. The remaining rows are policy
choices the design can absorb either way.

| Item | Question | State |
| --- | --- | --- |
| **Denomination & deposit scaling (A3)** | is a JAM balance unit a planck with the GP constants taken literally (`B_L = 1` ⇒ ~10⁻⁷ DOT per KB; the whole ~20 GB reserve ≈ 2 DOT) — or are `B_S`/`B_I`/`B_L` rescaled so a KB of footprint binds material value? Fixes the genesis chain-spec integer and decides whether the reserve is **real collateral or an accounting token**. The mechanism is invariant either way (token-denominated cap, §3.1); the market thesis is not. | economics call; **blocks the genesis figure and the product case**, not this design |
| **Reserve share of JAM-level supply** | what fraction of total JAM-level token supply does the reserve hold, and who else holds balances at genesis (validators, migrated pots, other services)? Steer from the source articles: *"all $JAMKB would be initially owned by the DOT DAO"* — scarcity by dominant supply share. To confirm: the **genesis inventory** — reserve `= cap − W` on the Parachain Service balance, working balance `= W` (the genesis release `released₀`, §3.1), **nothing else**. Every native balance outside the reserve is footprint capacity the cap does not govern. The Coretime plane's top-up source for hosted-parachain state is the largest recurring line in that inventory. | economics call; pairs with A3 |
| **Genesis sequencing & provenance of the endowment** | Two questions, in order. **First**: is JAM genesis a fresh chain spec (greenfield) or migrated relay-chain state? Upstream-dependent and unanswered: neither the Parachain Service design nor the cumulus-on-jam scope doc covers balance migration (the latter commits only to parallel client support). Under migration, the endowment is not creation but an allocation out of migrated supply — name the source of units (realistically the DAO treasury position) and the authorization that carves it out as part of the transition; entangled with A3 (under literal GP constants the whole endowment is ~2 DOT and provenance is a non-event; rescaled, it is a material diversion of DOT supply needing its own sequencing). **Second**: §3.1 step 1 assumes the Parachain Service's account exists **in the JAM chain spec** with `cap` on its balance. If the Parachain Service is instead instantiated post-genesis, the units (JAM has no mint) must sit with a named **interim custodian** account and reach the Parachain Service by an attested transfer — name the custodian and handover, or commit to Parachain-Service-at-genesis. | sequencing decision, now upstream-dependent (migration question); refines the endowment row below |
| **χM custody & gratis policy** | who holds the JAM-privileged manager service χM at and after genesis, and what is its policy on gratis grants — which add footprint capacity **outside the cap**? capacity reporting assumes χM grants none or reports all; χM custody is part of the trust base. | trust-base clarification; owned by DAO / platform governance |
| Voluntary-return permissionlessness | `OPEN(D-permissionless)`: may anyone re-enter units into Hub supply (market-elastic receipts), or only registered counterparties? Note the burn asymmetry: returns unlocking to holder accounts re-enter supply without a DOT burn; units re-entering DAO custody burn again on re-sale (Referendum 1926). | monetary-policy decision |
| Direct-release policy | `OPEN(D-release-variant)` **resolved in the mechanism**: the direct path is the default for all releases (§5.2); the escrowed variant is a per-allocation opt-in hardening, available once its verbs land (§8.2). Governance may still mandate escrow per allocation in the approving referendum. | mechanism decided; per-allocation policy with governance |
| Lease duration | are time-bounded leases required? Mechanism: `Allocation.expires_at: Option<BlockNumber>`, fixed with recovery terms by the approving referendum. The manager executes nothing autonomously and has no hooks — expiry is evaluated lazily at call time. Before expiry, forced reclaim is governance-only. After expiry: recovery begins with an operator- or governance-posted **wind-down notice** (the recipient-contact step, on-chain); recovery actions against the target — freeze first — are operator-executed under the referendum-fixed terms, **never permissionless** (freeze suspends a live service); only counterparty-neutral completion steps (settle, eject of an emptied target, sweep) are permissionlessly callable. | mechanism designed; product decision pending |
| `Allocation.conditions` semantics | confirm conditions are **off-chain / governance-interpreted only** — no on-chain condition engine is implied or planned | needs one confirming sentence |
| Excess disposition | governance procedure for donations / unattributed inflows (§6 `excess`) | undefined |
| Lease denomination under rate changes | are (paid) leases token-denominated (rate drops = silent lessee windfall, nothing to do) or KB-denominated (manager right-sizes after each deposit-rate change via `Reassign` — always CASH-feasible, since the same rate drop loosened the bound)? Affects leases only; released units are the holder's regardless | policy decision; mechanism exists either way |
| **Lease protection model** | Return is unenforceable in JAM's deposit model (no eviction; recovering the used slice requires the data deleted), so an unprotected lease sells permanent storage at rental price (adverse selection). Every lease requires, before delivery, exactly one of: (a) escrowed collateral ≥ sale price of everything credited — non-return = forfeit = completed sale, refund = min(paid, market); (b) enforced enumerable key schema + write-indexer from onboarding — enables §5.3 repossession. **Under Referendum 1926 all allocations are paid at market, so (a) collateral is the default**; the (b) key-schema path matters only if a future referendum re-opens subsidized allocations. Delivery: full up-front (paid allocations carry their own collateral). Pre-expiry freeze and reassignment are governance-gated, condition-breach only | policy decision; collateral default set by Ref 1926 |
| Initial JAM-side endowment | provenance of the reserve's genesis JAM balance | **answered in §3.1 (bootstrap) for the greenfield case only**: JAM chain-spec allocation of `cap` to the Parachain Service balance: `cap − W` as the tracked reserve line, `W` as the working balance (the genesis release `released₀`). Under a migrated-state launch, provenance and authorization are open (§3.1 migration caveat; sequencing & provenance row above). Genesis authorship is trust-free w.r.t. this design: the bootstrap attestation verifies the endowment content before thaw (fail closed), so deployer identity is irrelevant. What remains is the ratification process (OpenGov / Fellowship) that puts it in the chain spec, plus the sequencing & provenance row above. The attestation checks the reserve line only; Parachain Service code, self-supervision and χ privilege assignments are covered by ratification review of the whole genesis artifact, not by the thaw gate |

### 8.2 Upstream scope changes (Parachain Service and platform)

| Dependency | Status | Blocks |
| --- | --- | --- |
| **Remaining Asset-Hub-only upward-message extensions for supervision flows.** Landed: supervision-aware `TransferOut` (foreign `source`, per-side supervisor-balance selectors, typed `TransferFailed { id, error }` — source-scoped `InsufficientServiceBalance` doc-comment fixed) covering lease delivery and reassignment; the supervised-store verbs (Parachain Service design §3.3): `RemoveServiceStorage { service, key }` (= `CleanupStorage` pages, per-key) and `Forget { target: Target::Service, hash, len }` (= `ForgetPreimage`). Landed since the last audit: `EjectService`, `SetServiceSupervisor` (handover, self-release included), `CreateService` with the `ServiceCreation { id, result }` echo, and exhaustive typed failures (`ServiceEjectError` distinguishing not-supervised / created-this-slot / not-empty; `ServiceStoreFailed { service, error: UnknownService \| NotSupervised \| NotRequested }`) — the soundness condition for §7.3's absence-of-failure inference. Still missing: `SetServiceCode { target, code_hash, min_acc_gas, min_memo_gas }` (freeze, §5.3 Phase 1; GP Ω_U writes all three fields together, so the verb must carry all three; it must not inherit `UpgradeService`'s preimage-availability check, since accepting a preimage-free hash is the mechanism, and it must restore sane gas values on unfreeze); a drain-amount payout (`PayoutRegular`, §3.2 — `TransferOut` fixes `amount` at emission); the atomicity of `ConvertAndRelease` (`TransferOut` sup→reg plus `SetServiceSupervisor` in one accumulate invocation, all-or-nothing per the checkpoint note below). ~~Positive execution echo `Executed { id, effect }`~~ **dropped**: superseded by the §3.4 anchor-binding rule (prune-is-consume enforced Asset-Hub-side; an echo entry would be as prunable/evictable as the failure entry it replaces). **Two surviving exceptions**: `Ejected { id, target, swept_regular, swept_supervisor }` and `PaidOut { id, amount }` (both read via `info` in the executing invocation). These verbs move **state-determined amounts** that absence-of-failure cannot settle; without them the eject sweep is an unattributable credit and the §6 lease-return/excess split has no input | mostly landed; 1 verb + 2 echoes + a drain payout + 2 normative notes outstanding | §5.2/§5.3; §5.1 leases expressible today (freeze for the solicit-pinning race) |
| **Two normative notes in Parachain Service design §5.1** — (i) *head-write-commits-replay*: `head_data` (step 6) is written in the same checkpoint interval as, and immediately before, the upward-message replay (step 7), so an advanced head commits the replay; currently an accident of step ordering and an advisory "should checkpoint" — §7.3's settle condition 1 depends on it. (ii) *rank-2 eviction guarantee*: `AccumulateLog` entries are evicted only under rank-2 pressure — §7.3's residual-risk bound depends on it | ordering exists in text; needs normative status | §7.3 confirmations |
| Minimum-return floor on `incoming_transfers` — the self-funding queue silently absorbs returns below the per-entry cost (no bounce, no record) | behavior specified; floor value + optional recorded-rejection entry open | §5.4 attribution |
| Parachain Service ↔ #539 reconciliation (supervisor-balance awareness throughout) | **done** — the current Parachain Service design is supervision-aware and this document is written against it | — |
| Validation-inputs / state-proof spec | missing (SPEC_GAPS #1) | §7.3 confirmations; §5.4 inbound consumption |
| **In-PVF account-leaf reads — dropped entirely.** The formerly narrowed ask ((i) the reserve account leaf, (ii) target existence/`created`) is withdrawn. Both consumers are rare, governance-executed acts on public JAM-state evidence (§7.5 governance tier — genesis attestation, reserve re-anchor, `Burnt` write-off), so no in-PVF account-read path is required. SPEC_GAPS #1 needs to cover only Parachain Service state (the log and the transfer queue). Execution checks (supervision, reassignment bound, eject preconditions, `min_memo_gas`) run **Parachain-Service-side at replay time via GP `info` (Ω_I) — available today**, reported back as typed failures; reassignment retry sizing needs an `available` payload on `InsufficientServiceBalance` (upstream ask; the variant is fieldless today); lease-target monitoring is off-chain. Pending-transfer visibility exists nowhere as a provable input (JAM accumulation-queue state); the §5.3 drain guard is our own records plus the Phase 0 notice period | **withdrawn** — no upstream work | — |
| **Generic AH→JAM transport pallet** (`validate_block` output-via-host-functions rework) | design direction in [cumulus-on-jam](../cumulus-on-jam/cumulus-on-jam.md) §11; unimplemented. Three requirements from this design (§3.4): (i) restricted messages are pull-only — one runtime-named provider per message class, no public push API; the restricted list mirrors Parachain Service design §4.3; (ii) emission ordering — the message drain and `set_head` are written in the same irrevocable tail (the AH-side counterpart of the head-write-commits-replay note above); (iii) an inbound API exposing the inherent-verified validation inputs (log entries, incoming transfers) to pallets — shape unspecified; it determines the §7.3 validation-input store design. Inherited dependency: the child-PVF ABI is half-specified — the index registry landed (Parachain Service design §4.3: fixed-index imports; JAM host calls keep their Gray Paper indices, service-native host functions number from 100), the per-call argument encoding has not (SPEC_GAPS #6); every emitted message depends on the latter | §3.4 emission; §7.3 confirmations |
| GP §9.5 supervisors in implementations | merged in GP `f01d06d`; **zero implementations** (polkajam's `write` host-call is hardwired to the calling service — no foreign-state mutation exists anywhere yet) | §5 entirely |
| GP release tag containing §9.5 | pending — **v0.8.0 was tagged *before* #539 merged** (tag at `07f041d`; `f01d06d` is untagged `main`), so no released GP version contains supervisors; "GP 0.8.0 semantics" in any doc must be read as "post-0.8.0 `main`" | version pinning |
| **GP-level service purge ("tombstone eject")** — supervisor condemns a supervised service → expunge-period delay (~32 h, preserving preimage-availability discipline) → whole-record removal with footprint counters zeroed wholesale, balances swept. Implementable without key preimages: the owning service id is plaintext-interleaved in every state key, so full-node state iteration enumerates a service's entries; per-entry remove-by-hash is not viable (the deposit refund needs `key_len`, which state does not store) | Gray Paper proposal to be filed; philosophically contentious (JAM deliberately has no eviction) — **nothing in this design depends on it**; acceptance would remove the §5.3 key-knowledge precondition and the stranding class it creates | optional |

### 8.3 Internal technical decisions (owned by this design)

| Item | State |
| --- | --- |
| ~~`OPEN(D1-layout)` reserve layout~~ | **decided**: the reserve is a tracked line within the Parachain Service's own balance, above a configured segregation floor (§3.2); no compartment service |
| Gas budgeting per JAM verb | deferred-transfer gas limits (destination `min_memo_gas` ⇒ `LOW`), accumulate gas per verb, and who absorbs `LOW` failures |
| Parameters | confirmation window (§7.3 liveness assumption), `MaxJamOpsPerBlock` (§3.4) — **sized against a stated worst-case settle lag in blocks**, such that `MaxJamOpsPerBlock × lag × max-entry-size` stays under the 64 KiB rank-2 eviction threshold (§7.3 residual risk), with a log-occupancy alarm well below the cap; retention horizon (§7.1); wind-down notice period (§5.3 Phase 0); reconciliation cadence (§7.5); `MaxKeysPerPage` + partial-page semantics for `CleanupStorage` (byte-bound: 48 KiB digest share; gas-bound: worst case well under the 10M accumulate budget; revert-vs-prefix behavior chosen with the Parachain Service team) |
| **Write-capture indexer for lease targets** | instrumented full node capturing every foreign `write`/`solicit` at accumulate execution, from onboarding (keys exist transiently in every node's execution — polkajam already trace-logs them); replay-from-creation-timeslot as the retroactive fallback; indexer liveness monitored. Precondition only for the (b) key-schema protection path — relevant if a future referendum re-opens subsidized allocations (§8.1) |
| ~~Inert stub code~~ (freeze target) | **dropped** — freeze is SetCode to a *preimage-free* hash (§5.3 Phase 1): stronger (target can never execute), zero footprint cost, no eject-blocking request, nothing to define/audit/provision. No stub remains anywhere in the design |
| Operator set management | count, rotation, compromise recovery — authorization surface, especially with post-expiry operator powers |
| `OPEN(D10)` memo registry | finalize after D-permissionless (8.1) |
| Parachain Service upgrade pause policy | `OPEN` operational policy, one paragraph |
| ABI / error model / call-path tables | not specified (§3.2); mechanical |
| pallet-assets administration precompile | to be built (general-purpose, reusable) — §3.3 |
| Custom OpenGov origins runtime interface | excluded from MVP (Root and registered operators suffice) |

---

## 9. References

1. [Referendum 1926](https://polkadot.polkassembly.io/referenda/1926): "100% of
   DOT revenue from JAMKB sales to be burned" (executed): protocol-level burn of
   all DAO proceeds (non-DOT auto-converted), and no JAMKB grants, gifts, or
   below-market loans.
2. JAM Gray Paper, `main` @ [`f01d06d`](https://github.com/gavofyork/graypaper/commit/f01d06d5ca6aa10a4a123e185f57f18df908eb12):
   §9.3 *Account Footprint and Threshold Balance*, §9.4 *Service Privileges*,
   §9.5 *Supervisors*, §12 *Accumulation*
   (burn: [accumulation.tex#L179](https://github.com/gavofyork/graypaper/blob/f01d06d5ca6aa10a4a123e185f57f18df908eb12/text/accumulation.tex#L179)),
   App. B (ΨA, host-calls).
3. [Parachain Service on JAM](../parachain-service-on-jam/parachain-service-on-jam.md):
   §4.3 (`send_upward_message` and the `UpwardMessage` ABI), §5.1 (incoming
   transfers, self-funding queue), §5.4 (service self-upgrade via Asset Hub),
   §6.1 (state-balance accounting, Coretime chain), §3.3 (supervised-service
   verbs).
4. [SPEC_GAPS.md](https://github.com/paritytech/parachain-service/blob/master/SPEC_GAPS.md):
   validation-inputs gap (#1).
5. [Cumulus on JAM](../cumulus-on-jam/cumulus-on-jam.md): §8 (inherent/host-function
   inputs, validation-inputs gap), §11 (`validate_block` rework: outputs via host
   functions).
6. *DOT DAO and the need for $JAMKB*; *DOT DAOism under JAM: An Island Story*
   (Medium, 2026).
