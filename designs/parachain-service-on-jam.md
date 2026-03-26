# Parachain Service on JAM

> **Status**: Draft  
> **Last Updated**: 2026-03-02

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [The Parachain Service](#3-the-parachain-service)
   - 3.1 [Service State Layout](#31-service-state-layout)
   - 3.2 [Work Items](#32-work-items)
   - 3.3 [Refine Result](#33-refine-result)
4. [Refine: In-Core Execution](#4-refine-in-core-execution)
   - 4.1 [What Refine Does](#41-what-refine-does)
   - 4.2 [PVF Execution in PVM](#42-pvf-execution-in-pvm)
   - 4.3 [Host Functions & PVM Imports](#43-host-functions-pvm-imports)
5. [Accumulate: On-Chain Integration](#5-accumulate-on-chain-integration)
   - 5.3 [Code Upgrade Lifecycle](#53-code-upgrade-lifecycle)
6. [Authorization & Coretime](#6-authorization-coretime)
   - 6.3 [Authorizer Design: AURA Example](#63-authorizer-design-aura-example)
   - 6.4 [On-Demand Parachains](#64-on-demand-parachains)
7. [Messaging](#7-messaging)
8. [Open Questions](#8-open-questions)
9. [Missing JAM / Gray Paper Features](#9-missing-jam-gray-paper-features)
10. [References](#10-references)

---

## 1. Overview

This document describes the architecture of the **Parachain Service** — a JAM service that implements
Polkadot's parachain host functionality. The Parachain Service is the JAM successor to the current
Polkadot relay-chain parachain host, mapping all the concepts of collation, validation, availability,
and finality into JAM's Collect-Refine-Join-Accumulate (CRJA) computation model.

The key conceptual mapping from today's Polkadot to JAM:

| Polkadot 1.x | JAM | Role |
|---|---|---|
| Collation (collator builds candidate + PoV) | **Collect** | Gather inputs off-chain |
| Backing (validator group checks PoV) | **Refine** | Stateless off-chain validation |
| Availability + Approval | **Guarantees + Assurances** | Attest correctness, ensure data available |
| Inclusion + on-chain parachain consensus | **Accumulate** | Integrate results into shared state |

### Scope

This document covers:

- How a parachain block's lifecycle maps onto JAM's Work Package / Refine / Accumulate model
- The service state layout and key data structures
- How authorization and coretime allocation integrate
- Cross-chain messaging under the new model

This document does **not** cover JAM fundamentals in depth; readers are assumed to be familiar with
the [JAM Gray Paper](https://graypaper.com) concepts (services, work packages, refine, accumulate,
guarantors, etc.).

---

## 2. Architecture Overview

The Parachain Service maps the current relay chain's parachain host logic onto JAM's
two execution domains:

- **Refine (in-core)**: Executes `validate_block` — the PVF validation that backing
  validators currently perform. Guarantors run the PVF against the PoV to verify the
  parachain block candidate. This replaces the current backing subsystem.
- **Accumulate (on-chain)**: Performs candidate enactment — updating head data, processing
  signals, managing channels and code upgrades. This replaces the current inclusion pallet
  logic (`enact_candidate`).

```
 ┌──────────────────────────────────────────────────────────────────────┐
 │                         JAM Chain                                    │
 │                                                                      │
 │  On-Chain    ┌────────────────────────────────────────────────────┐  │
 │              │              Services Layer                        │  │
 │              │  ┌──────────────────────┐   ┌──────────────────┐  │  │
 │              │  │  Parachain Service   │   │  Other Services  │  │  │
 │              │  │  ┌────────────────┐ │   │                  │  │  │
 │              │  │  │  accumulate()  │ │   │  accumulate()    │  │  │
 │              │  │  └────────────────┘ │   │                  │  │  │
 │              │  └──────────┬───────────┘   └──────────────────┘  │  │
 │              └─────────────┼────────────────────────────────────┘  │
 │  ·····················│····│····································   │
 │  In-Core              │    │                                      │
 │              ┌─────────────┴───────────┐                          │
 │              │  Parachain Service       │                          │
 │              │  ┌────────────────┐     │                          │
 │              │  │   refine()     │     │                          │
 │              │  └────────────────┘     │                          │
 │              └─────────────────────────┘                          │
 │              ▲                                                     │
 │              │ Guarantors execute Refine on assigned cores         │
 │                                                                      │
 │  ┌────────────────────────────────────────────────────────────────┐  │
 │  │    Data Availability (erasure-coded)                           │  │
 │  └────────────────────────────────────────────────────────────────┘  │
 └──────────────────────────────────────────────────────────────────────┘
```

The CRJA pipeline for a parachain block:

```
[Collect]     Collator gathers transactions, builds a parachain block candidate and puts it into a work package.
    │
    ▼
[Refine]      IN-CORE: Guarantors execute the parachain service refine functionality. Internally this calls the PVF code to verify the work package.
              Stateless, off-chain, metered via PVM gas.
              Output: per-item work-digests plus authorization/export metadata, assembled into a Work Report.
    │
    ▼
[Join]        The Work Report (aggregating per-item work-digests and authorization metadata) is submitted on-chain.
              JAM validators attest (guarantee) its correctness. Availability is ensured.
    │
    ▼
[Accumulate]  ON-CHAIN: The Parachain Service's Accumulate function runs on-chain.
              It records the new parachain head, processes upward signals (code upgrades,
              HRMP channel management, outbound transfers) and queues downward signals for parachains.

```

### Responsibility Split: JAM Native vs. Parachain Service

A key aspect of the migration is that many relay chain responsibilities are absorbed by JAM
natively. The Parachain Service only implements what is specific to the parachain protocol:

| Responsibility | Polkadot 1.x (Relay Chain) | JAM | Owner |
|---|---|---|---|
| PVF execution (validation) | Backing subsystem + PVF executor | Refine entry point executes PVF in child PVM | **Parachain Service** |
| Candidate availability tracking | `inclusion` pallet (bitfields, thresholds) | Guarantee + Assurance mechanism | **JAM native** |
| Approval voting | Approval voting subsystem | Auditing + assurance protocol | **JAM native** |
| Dispute resolution | `disputes` pallet + dispute coordinator | Judgment mechanism | **JAM native** |
| Head data & state tracking | `inclusion` + `paras` pallets | Accumulate entry point | **Parachain Service** |
| Code upgrades | `paras` pallet (PVF pre-checking) | Accumulate + preimage store | **Parachain Service** |
| Coretime accounting | `coretime` + `on_demand` pallets | Core assignment + usage tracking | **JAM native** |
| Data availability | Availability distribution subsystem | Erasure-coded DA layer | **JAM native** |

---


## 3. The Parachain Service

The Parachain Service is a JAM service whose code implements the on-chain logic of the parachain
host. It holds all per-parachain state and drives the CRJA pipeline for every registered parachain.

The Parachain Service is expected to be an **always-accumulate** service in the Gray Paper sense.
Even in blocks where no parachain candidate becomes available, it still needs an accumulation step
to apply privileged control-plane updates such as authorizer queue changes, validator-key updates,
and other service-level bookkeeping that must take effect without waiting for a parachain block.

### 3.1 Service State Layout

The service state is a key-value store. Logically it contains:

```rust
// Top-level service state (conceptual, not final)
struct ParachainServiceState {
    /// All registered parachains and their current metadata.
    parachains: Map<ParaId, ParaInfo>,

    /// Channel state (open channels, message metadata).
    /// The exact structure depends on the final XCMP design — with full XCMP,
    /// only message hashes and channel metadata are stored on-chain, not payloads.
    channels: Map<ChannelId, Channel>,

    /// Downward signal queues (from Parachain Service → parachain).
    dsq: Map<ParaId, VecDeque<DownwardSignal>>,

    /// PVF (Parachain Validation Function) preimage registry.
    pvf_registry: Map<ValidationCodeHash, PvfEntry>,

    /// Stores when pending upgrades timeout. 
    pending_upgrades_timeouts: Map<Timeslot, Vec<(ValidationCodeHash, ParaId)>>,
}

struct PvfEntry {
	/// Length of the solicited preimage, used when forgetting it later.
	code_len: u32,
	/// Number of parachains currently referencing this validation code.
	/// When this drops to zero the preimage can be released immediately.
	ref_count: u32,
}

struct ParaInfo {
    /// Current head data (output of last included block).
    head_data: HeadData,
    /// Hash of the currently active validation code.
    validation_code_hash: ValidationCodeHash,
    /// Pending code upgrade, if any. See §5.3 for the full lifecycle.
    pending_upgrade: Option<(ValidationCodeHash, Timeslot)>,
}
```

### 3.2 Work Items

Each work package submitted to the Parachain Service contains one or more **work items**. For the
Parachain Service, a work item corresponds to one parachain candidate:

```rust
/// A work item for the Parachain Service encodes a single parachain candidate.
struct ParachainWorkItem {
    /// The parachain this candidate belongs to.
    para_id: ParaId,

    /// The hash of the currently active validation code. Used by Refine to
    /// look up the PVF bytecode from the preimage store.
    validation_code_hash: ValidationCodeHash,

    /// The Proof-of-Validity (PoV) — the actual block data + witness.
    /// This is the large input to Refine (up to ~15 MB per slot across all items).
    pov: Vec<u8>,
}
```

Initially, each work package will contain a single work item (one parachain candidate).
Support for multiple items per package may be added later.

### 3.3 Refine Result

The Parachain Service's Refine function returns an opaque result blob per work item. This blob is forwarded to `accumulate`.
From the service's perspective, Refine either succeeds or fails:

```rust
/// The Parachain Service's Refine output for one parachain candidate.
enum ParachainWorkResult {
    Ok {
        /// New head data produced by the parachain block.
        head_data: HeadData,
        /// Upward signals from the parachain (code upgrades, channel ops, transfers).
        upward_signals: Vec<UpwardSignal>,
        /// Other message related fields may need to be added, depends on the messaging approach.
    },
    /// PVF execution failed (e.g. invalid PoV, bad state proof, panic).
    ///
    /// Opaque error payload, max 1024 bytes.
    Err(BoundedVec<u8, 1024>),
}
```

Both variants are encoded into the Refine result blob — from JAM's perspective, Refine
always succeeds. The `Ok`/`Err` distinction is internal to the Parachain Service and
decoded by Accumulate. This avoids losing error context to JAM's fixed `workerror` enum,
which carries no payload. The combined size of all result blobs plus the authorizer trace
in a work-report is limited to **48 KiB** by the Gray Paper.

- **`Ok`** is returned when PVF validation succeeds without error.
- **`Err`** is returned when PVF validation fails. The opaque error payload (up to 1024
  bytes) is stored by Accumulate and can be read by the parachain later. This can be used
  for example to slash a collator who claimed an authorizer slot that was not theirs, e.g.
  by building blocks in consecutive slots that should have belonged to different collators.

---

## 4. Refine: In-Core Execution

### 4.1 What Refine Does

The Refine entry point of the Parachain Service executes the **Parachain Validation Function
(PVF)** for each work item. This is a stateless, in-core computation performed by guarantors
(validators assigned to the relevant JAM core).

Refine:
1. Fetches the PVF bytecode via `historical_lookup` (using `validation_code_hash`).
2. Instantiates a child PVM with the PVF.
3. Executes the PVF against the PoV (the `validate_block` call).
4. Returns a `ParachainWorkResult` with the committed outputs.

Because Refine is stateless, it cannot write to service storage.

### 4.2 PVF Execution in PVM

PVFs execute in the **Polkadot Virtual Machine (PVM)**, a RISC-V based VM. Resource limits
(gas, memory, code size) are defined in the Gray Paper and not repeated here.

### 4.3 Host Functions & PVM Imports

On JAM, PVFs execute inside a child PVM instance spawned by the Parachain Service's Refine
function. **Hashing**, **trie operations**, and **signature verification** are expected to
move into PVM guest code — transpilation to native code should bring acceptable performance,
though benchmarks are needed to confirm exact numbers.

The Parachain Service's Refine function provides the following host functions to the child
PVM executing the PVF:

| Host function | Purpose |
|---|---|
| `lookup(hash)` | Fetch a preimage from the Parachain Service's preimage store (e.g. PVF code) |
| `foreign_lookup(service, hash)` | Fetch a preimage from another service's preimage store |
| `export(data)` | Write a segment to the JAM Data Lake (e.g. outbound XCMP payloads) |
| `gas()` | Query the remaining gas budget |
| `refine_context()` | Access the refinement context (anchor, lookup-anchor, prerequisites) |
| `work_package()` | Access the work package metadata |
| `work_item_payload()` | Access the current work item's payload (the PoV) |
| `send_upward_signal(signal)` | Emit an upward signal (code upgrade, channel ops, transfers) |
| `read_downward_signals()` | Read pending downward signals for this parachain |

---

## 5. Accumulate: On-Chain Integration

### 5.1 What Accumulate Does

Once a work report has been guaranteed and its data is available, JAM invokes the
**Accumulate** entry point of the Parachain Service. This runs on-chain with full access to
service storage, roughly ~10ms of PVM gas per work result.

Accumulate for the Parachain Service performs the following operations, in order. These
correspond directly to the relay chain's current `enact_candidate` logic in the inclusion
pallet (`polkadot/runtime/parachains/src/inclusion/mod.rs`):

1. **Head data update**: Writes the new `head_data` from the candidate commitments into
   `ParaInfo` for the parachain, and records the relay-parent context.
2. **Upward signal processing**: Processes `UpwardSignal`s from the candidate commitments:
   - `RequestCodeUpgrade` — calls `solicit(new_code_hash, code_len)` to request the
     preimage via the JAM preimage store, sets `pending_upgrade` with a deadline, and
     begins the dual-code transition period. See §5.3 for the full lifecycle.
   - `InitOpenChannel` / `AcceptOpenChannel` / `CloseChannel` — manages HRMP channel
     lifecycle and queues corresponding `DownwardSignal`s to the affected parachains.
   - `TransferOut` — calls `transfer()` to the destination JAM service (e.g. Asset Hub).
3. **Downward signal pruning**: Removes consumed downward signals from the parachain's
   signal queue, based on the `processed_downward_signals` count in the candidate commitments.
4. **HRMP watermark advancement**: Prunes inbound HRMP messages up to the watermark declared
   in the candidate commitments, freeing channel capacity.
5. **Outbound HRMP queuing**: Enqueues horizontal messages from the candidate commitments into
   the destination parachain's HRMP channel, updating channel metadata (message count, total
   size, MQC head hash).
6. **Code upgrade transition check**: Checks whether the candidate was validated with
   a pending new PVF code. If so, activates the new code (updates
   `ParaInfo.validation_code_hash`), calls `forget()` on the old code hash, and
   clears `pending_upgrade`. Also checks if a pending upgrade has exceeded its deadline
   without the preimage becoming available or without any block using the new code — if
   so, rejects the upgrade by calling `forget()` on the new code hash and clearing
   `pending_upgrade`.
7. **Incoming transfer processing**: Any `OnTransfer` calls received from other JAM services
   are decoded (memo identifies the target `ParaId`) and queued as `DownwardSignal::IncomingTransfer`
   for the destination parachain.

Compared to the relay chain's inclusion pallet, the Accumulate function is notably simpler.
Several responsibilities that the relay chain handles are instead managed by JAM natively:

- **Availability tracking**: JAM's guarantee and assurance mechanism tracks availability
  votes and determines when a candidate is available — the Parachain Service does not need
  to maintain `PendingAvailability` state or process availability bitfields.
- **Dispute resolution**: JAM's judgment mechanism handles dispute escalation, slashing,
  and finality delays at the protocol level. The Parachain Service only needs to handle
  rollback if notified of an invalid judgment (handled by JAM's native judgment mechanism).
- **Validator rewards**: Backing and availability attestation rewards are handled by JAM's
  validator reward mechanism, not by the Parachain Service.

In practice, the core Accumulate logic is primarily **parachain bookkeeping**: updating head
data, managing message queues, and tracking code upgrades. The heavy lifting of consensus
and security is delegated to JAM.

### 5.2 Gas Budget

The Accumulate gas budget per work result is tight (~10ms). Complex message processing or large
HRMP queues must be handled carefully. Options:

- **Batched processing**: Process only a bounded number of messages per Accumulate call; leave
  remainder for the next slot.
- **Deferred actions**: Schedule expensive operations (e.g. code upgrade application) for a
  future Accumulate call.
### 5.3 Code Upgrade Lifecycle

Runtime (PVF) code upgrades follow a well-defined lifecycle using JAM's preimage
store (`solicit`/`provide`/`forget`) and the `xtpreimages` block extrinsic.

```
Phase 1: Request
  Parachain includes UpwardSignal::RequestCodeUpgrade { new_code_hash }
  in its candidate commitments.
      │
      ▼
Phase 2: Rquest Preimage
  Accumulate calls solicit(new_code_hash, code_len).
  Sets pending_upgrade with a deadline (current timeslot + UPGRADE_TIMEOUT).
  The parachain now pays for TWO PVF codes in the preimage store.
      │
      ▼
Phase 3: Preimage Submission
  Anyone (collator, block author, third party) can submit the PVM code directly to JAM.
      │
      ▼
Phase 4: Transition Period
  Once the preimage is available, collators MAY build blocks using either
  the old or the new PVF code. Refine accepts both validation_code_hash
  and pending_upgrade.new_code_hash during this window.
  The parachain runtime itself can check preimage availability (via the
  Parachain Service state exposed through the validation inputs) and
  trigger the switch from within its own block execution — no
  service-side polling is needed.
      │
      ▼
Phase 5: Activation or Rejection
  (a) First block using new code: Accumulate detects the candidate was
      validated with new_code_hash. It:
      - Sets validation_code_hash = new_code_hash
      - Calls forget(old_code_hash, old_code_len) to release the old code
      - Clears pending_upgrade
      The transition is complete. Only the new code is accepted from now on.

  (b) Deadline exceeded: If the deadline (set in Phase 2) passes without
      the preimage becoming available or without any block using the new
      code, Accumulate rejects the upgrade:
      - Calls forget(new_code_hash, code_len) to release the new code
      - Clears pending_upgrade
      The parachain continues with the old code.
```

**Key properties:**

- **No pre-checking needed**: PVM has no compilation bomb risk (unlike WASM), so there is
  no pre-checking vote. The code is accepted as soon as the preimage is available.
- **Dual-code cost**: During the transition period, the parachain pays for both the old
  and new PVF code in the preimage store. This incentivizes timely adoption.
- **Permissionless submission**: The preimage can be submitted by anyone — the collator,
  block author, or any third party. The JAM protocol validates the hash against the
  solicitation.
- **Timeout protection**: The deadline prevents parachains from indefinitely occupying
  preimage store space with unused code. `UPGRADE_TIMEOUT` should be long enough for
  collators to update their software (e.g. 24-48 hours).
- **No active polling needed**: The service does not need a background polling mechanism for
  pending upgrades. Upgrade state is checked when a candidate for that parachain is accumulated,
  and the always-accumulate control path can additionally clear timed-out upgrades as part of
  normal service execution.
---

## 6. Authorization & Coretime

JAM natively handles core assignment and coretime tracking, but the Parachain Service still needs
to decide **which authorizers are valid for each core**. For now, that control-plane function is
best modeled as an integration with the **Coretime Chain** (today's `pallet-broker` / coretime
system chain), which already owns the notion of who has rights over a core.

The important point is not the internals of authorizer execution, but the ownership boundary:

- The **Coretime Chain** decides which parachain or customer owns a core and therefore which
  authorizer queue should be installed for that core.
- The **Parachain Service** applies those decisions to JAM via its always-accumulate control path.
- JAM's `is_authorized` invocation then checks a work-package token against one of the authorizers
  currently in the core's authorizer pool.

This yields the following high-level flow:

```
Coretime Chain (pallet-broker)
    │  decides which parachain/customer controls core N
    │  and computes the desired authorizer queue for that core
    ▼
Parachain Service (always-accumulate control path)
    │  applies the queue update via JAM assign(core, queue, next_assigner)
    ▼
JAM authorizer pool / queue
    │  rotates authorizers from the queue into the pool over time
    ▼
Guarantors
    │  call is_authorized() before Refine
    ▼
JAM chain (Accumulate)
```

Two Gray Paper details matter here:

1. `assign` **overwrites the entire authorizer queue for a core immediately**.
2. The queue length is **80 entries** and the pool length is **8 entries**. Updating the queue is
   immediate, while actual authorizer eligibility changes gradually as the pool rotates entries in
   from the queue.

For the initial design we should therefore assume a simple model: the Coretime Chain (or a service
acting on its behalf) decides the full next 80-entry queue and the Parachain Service applies it
directly. If later we want delayed queue-rollover semantics, that should be specified explicitly as
service-level logic rather than assumed from JAM itself.

### 6.3 Authorizer Design: AURA Example

The authorizer is a single piece of PVM code (≤ 64 KB) deployed once as a preimage and
reused across all cores. Per-core behavior is controlled by the **config blob** (`pf`),
which is committed to when the authorizer queue is set via `assign`.

#### Config

The config encodes the parachain's collator set and slot timing:

```rust
struct AuthorizerConfig {
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
the pool and queue as long as the collator set and slot duration remain unchanged.

When a parachain wants to **rotate its collator set** or **change its slot duration**, it
announces this to the Coretime Chain. The Coretime Chain then causes the Parachain Service's
always-accumulate path to apply a new authorizer queue for the relevant core, producing a new
authorizer hash from the same code and updated config.

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

1. Decode config (`pf`) → `collator_set_root`, `collator_set_size`, `slot_duration`.
2. Decode token (`pj`) → `collator_proof`, `collator_key`, `signature`.
3. Read the **anchor timeslot** from the refinement context. If JAM does not yet expose it
   directly, this becomes a required host-interface extension (see §9).
4. Compute the expected collator index:
   `collator_index = (anchor_timeslot / slot_duration) mod collator_set_size`.
5. Verify `collator_proof` against `collator_set_root` at leaf `collator_index`,
   confirming `collator_key` is the expected collator for this slot.
6. Verify `signature` over the work package hash using `collator_key`.
7. Return a trace carrying the collator identity (or other minimal authorization metadata)
   needed by Refine/Accumulate.

#### Anchor Selection and Slot Claiming

The collator picks an anchor block (one of the last 8 JAM blocks) whose timeslot maps
to their collator index. Since the authorizer pool holds O = 8 entries — all with the
**same** authorizer hash — the collator can pick any of the 8 recent anchors.

This has a consequence: for **small collator sets** (< 8 collators), a collator could
claim **two consecutive blocks** by choosing different anchor blocks whose timeslots
both map to their index (e.g. with `collator_set_size = 4` and `slot_duration = 1`,
anchor timeslots T and T+4 both yield the same collator index).

This is acceptable — the Refine logic enforces that the **anchor timeslot is
non-decreasing** compared to the parachain's last included block. This prevents
a collator from going backwards, but allows the same collator to produce consecutive
blocks when the set is small. The non-decreasing constraint is not strict (equal is
allowed) to support **elastic scaling**, where multiple blocks for the same parachain
may reference the same anchor.

#### Collator Set Rotation Flow

```
Parachain runtime
    │  Decides to rotate collator set (e.g. via session change)
    │  Sends XCM to Coretime Chain with new collator set root + size
    ▼
Coretime Chain
    │  UpwardSignal::SetAuthorizerQueue { core, authorizers }
    │  (new authorizer hashes computed from same code + updated config)
    ▼
Parachain Service (Accumulate)
    │  assign(core, new_queue)
    │  New authorizer hashes enter the pool via queue rotation
    ▼
Pool (8 entries)
    │  Old authorizer hashes drain out over ~8 blocks (48s)
    │  New ones rotate in
```

### 6.4 On-Demand Parachains

On-demand parachains (currently acquired via `pallet-on-demand`) continue to work: they
acquire a single-shot coretime allocation for one slot, submitted as a work package to the
Parachain Service for that slot only. This should remain **Coretime-Chain controlled** rather than
become an internal Parachain Service market. In other words, the Coretime Chain is responsible for
deciding who temporarily controls a core and for installing the corresponding authorizer queue.

One plausible extension is a fast-path market where an off-chain seller pre-registers authorizers
for short-lived slots and then resells access off-chain, but that is still conceptually a
Coretime-Chain policy question, not part of the Parachain Service itself.

---

## 7. Messaging

### 7.1 Current Limitations

Today, HRMP (Horizontal Relay-routed Message Passing) routes all inter-parachain messages
through the relay chain, with a practical limit of ~1 MB per channel per block. UMP (Upward
Message Passing) similarly routes messages from parachain to relay chain.

### 7.2 Full XCMP on JAM

The current HRMP model — routing full message payloads through the relay chain — cannot
work on JAM because the work result output is too small to carry message payloads on-chain.
Off-chain messaging is required.

JAM enables **full XCMP** (the original design): only message *headers* and *hashes* are
recorded on-chain; the actual message payloads are distributed off-chain via JAM's data
availability layer (D3L). This removes the per-message size bottleneck. The Refine function
uses `export()` to write outbound message payloads into DA segments, and Accumulate only
records the message hashes and channel metadata on-chain.

This also enables **speculative messaging**: since message payloads are in DA segments
(available for 28 days), recipient parachains can optimistically consume messages before
they are finalized, as proposed in the speculative messaging design.

For the Parachain Service:

- **Outgoing XCMP messages** from a parachain are included in the candidate commitments.
  During Refine, their payloads are exported to DA segments via `export()`. Accumulate
  records the payload hashes and updates channel metadata in the Parachain Service state.
- **Incoming XCMP messages** are fetched by the recipient parachain's collators directly
  from the DA layer by segment hash, keyed by the message hash recorded on-chain.

### 7.3 Upward & Downward Signals (UMP / DMP Replacement)

In Polkadot 1.x, UMP and DMP carry XCM programs between parachains and the relay chain.
On JAM, these are replaced by a **typed signal protocol** — no XCM is involved. Signals
are simple, well-defined message types that the Parachain Service processes directly.

#### Upward Signals (Parachain → Parachain Service)

Upward signals are included in the parachain's `CandidateCommitments` and processed by
Accumulate. They replace UMP's current role as a general-purpose XCM transport.

```rust
enum UpwardSignal {
	/// Request a runtime (PVF) code upgrade. Only the code hash is included.
	/// Accumulate calls `solicit(new_code_hash, code_len)` to request the preimage
	/// via the JAM preimage store. Anyone can then submit the code blob as a
	/// `xtpreimages` block extrinsic. See §5.3 for the full upgrade lifecycle.
	RequestCodeUpgrade { new_code_hash: ValidationCodeHash },
	/// Request to open an HRMP channel to another parachain.
	InitOpenChannel { recipient: ParaId, max_capacity: u32, max_message_size: u32 },
	/// Accept a pending HRMP channel open request.
	AcceptOpenChannel { sender: ParaId },
	/// Request to close an existing HRMP channel.
	CloseChannel { sender: ParaId, recipient: ParaId },
	/// Transfer DOT to Asset Hub. Accumulate calls `transfer()` to the
	/// Asset Hub service with the memo as payload. Only Asset Hub is a valid
	/// destination — parachain-to-parachain DOT transfers are handled
	/// internally by Asset Hub, not by the Parachain Service.
	TransferToAssetHub { amount: Balance, memo: Vec<u8> },
	/// Update the authorizer queue for a core. Only callable by the Coretime
	/// Chain. Accumulate calls the JAM `assign` host call (ΩA) to set the
	/// authorizer queue for the given core. The `assign` host call takes a
	/// core index, a sequence of authorizer code hashes (the queue), and
	/// validates that the calling service is assigned to that core.
	/// The Parachain Service must be listed in χA for the relevant core.
	SetAuthorizerQueue { core: CoreIndex, authorizers: Vec<AuthorizerHash> },
	/// Set the next epoch's validator key set. Only callable by the Staking
	/// Chain. Accumulate calls the JAM `designate` host call (ΩD) to set ι
	/// (the upcoming validator keys). The Parachain Service must hold the
	/// χV privilege for this call to succeed.
	SetValidatorKeys { keys: Vec<ValidatorKey> },
}
```

#### Downward Signals (Parachain Service → Parachain)

Downward signals are queued in the Parachain Service state and delivered to the
parachain as part of its next candidate's input data (replacing the DMP queue).

```rust
enum DownwardSignal {
	/// Notification that another parachain wants to open an HRMP channel.
	HrmpOpenRequest { sender: ParaId, max_capacity: u32, max_message_size: u32 },
	/// Notification that the recipient accepted the HRMP channel.
	HrmpAccepted { recipient: ParaId },
	/// Notification that a channel is being closed.
	HrmpClosing { initiator: ParaId, sender: ParaId, recipient: ParaId },
	/// DOT received from an external JAM service (e.g. Asset Hub) via
	/// `OnTransfer`. The source is always an external service, never another
	/// parachain — inter-parachain transfers are handled by Asset Hub directly.
	IncomingTransfer { source: ServiceId, amount: Balance, memo: Vec<u8> },
}
```

### 7.4 XCM

XCM is **not used** for upward/downward signaling between parachains and the Parachain
Service. XCM remains the message format for **inter-parachain** communication via XCMP
(§7.2) — parachain-to-parachain messages continue to be XCM programs. The transport
changes (HRMP → full XCMP via DA), but the XCM format itself is unaffected.

---

## 8. Open Questions

The following questions are not yet resolved and require further design work or community input:

1. **Coretime and authorization**: JAM natively handles core assignment and coretime tracking.
   The Parachain Service provides an **authorizer** — service-specific code that JAM calls
   to validate work packages before Refine. The authorizer verifies that the submitting
   parachain holds valid coretime for the requested core. The exact interface between the
   Coretime Chain and the Parachain Service's authorizer state needs to be specified.

2. **Accumulate gas budget**: Is ~10ms sufficient for all the Accumulate logic (head update,
   message processing, code upgrades)? The current relay chain performs more work on inclusion
   than this budget allows. Mitigation strategies (batching, deferral) need to be specified.

3. **Dispute integration**: JAM's judgment mechanism operates at the work-report level, not the
   individual-candidate level. How does the Parachain Service's per-para dispute semantics map
   onto JAM's per-work-report judgment? What happens if one candidate in a multi-candidate work
   report is disputed?

4. **`UPGRADE_TIMEOUT` value**: The code upgrade lifecycle (§5.3) requires a timeout after
   which an unused upgrade is rejected. The appropriate value depends on the expected
   collator update cadence and preimage store cost model. Candidates: 24-48 hours.

5. **Parachain registration & governance**: Today, parachains are registered via on-chain
   governance (Polkadot OpenGov). In JAM, what is the registration mechanism for the Parachain
   Service? Is it a `ServiceCreation` operation, or does the Parachain Service maintain its own
   internal governance?

6. **Finality guarantees during the judgment window**: Polkadot parachains currently offer
   ~1-minute finality. JAM's 1-hour judgment window means finality of parachain blocks is
   technically delayed. How do parachains and ecosystem tooling communicate this changed
   finality model?

---

## 9. Missing JAM / Gray Paper Features

The current design assumes two pieces of context that are not yet clearly exposed by the Gray Paper
host interface and therefore likely need either specification work or an explicit embedding into the
Parachain Service protocol:

1. **Anchor timeslot access**: the authorizer wants direct access to the anchor block's timeslot in
   order to derive the expected collator index. If this is not provided in the refinement context,
   JAM likely needs a dedicated host function or an equivalent context field.
2. **Lookup-anchor posterior state root access**: Parachain validation flows will need the
   posterior state root associated with the lookup anchor, not just its hash and timeslot. If that
   root is required, the refinement context or historical-lookup interface needs to expose it.

These are hard requirements, not optional refinements. The authorizer needs anchor-timeslot access
in order to derive the expected collator slot, and parachain validation is likely to need the
lookup-anchor posterior state root for state-proof reuse and retry scenarios where an earlier work
package failed to make it on-chain despite having a reusable PoV.

---

## 10. References

- [JAM Gray Paper](https://graypaper.com) — Formal JAM specification (Gavin Wood)
- [CoreJAM RFC #31](https://github.com/polkadot-fellows/RFCs/pull/31) — Original CoreJAM RFC
- [RFC-1: Agile Coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md)
- [RFC-5: Coretime Interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md)
- [Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
- [Polkadot Wiki: JAM Chain](https://wiki.polkadot.network/docs/learn-jam-chain)
- [Demystifying JAM](https://blog.kianenigma.com/posts/tech/demystifying-jam/) — Kian Paimani
- [JAM PVM Common API](https://docs.rs/jam-pvm-common/latest/jam_pvm_common/) — Host call specifications for Refine and Accumulate
- [JIP-1: Log Host Call](https://github.com/polkadot-fellows/JIPs/blob/main/JIP-1.md) — PVM logging specification
