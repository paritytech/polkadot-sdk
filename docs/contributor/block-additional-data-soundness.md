# Block Additional Data: Soundness and Design

## Overview

This document verifies the soundness of the block-additional-data mechanism, addressing the
GitHub issue's explicit request: "the idea needs to be verified that it is sound." We cover
seven critical properties: data availability, PoV/block size budgets, pruning/archive
interaction, non-executing import paths, host-function/runtime-upgrade compatibility,
malicious-peer handling, and header-decode compatibility for un-upgraded nodes.

Each point cites concrete mechanisms by file:line or exact behavior, never hand-waving.

## 1. Data Availability: Who Must Serve the Data, and What Happens If No Peer Has It

**Mechanism:** The additional data is stored in a new `sc-client-db` column
(`substrate/client/db/src/lib.rs:466-490`, constant `ADDITIONAL_DATA = 13`), persisted
alongside the block body. Sync fetches it via a new `BlockAttributes::ADDITIONAL_DATA` bit
(`substrate/client/network/common/src/sync/message.rs:42-72`), which a requesting node sets
when it needs the data.

**Chain-Level Policy:** A chain that opts into this feature (by depositing a
`DigestItem::AdditionalData` hash in its block headers) must document its own data
availability guarantees. The mechanism itself is generic and does not enforce a policy:

- If a chain requires the data to be available (e.g., for validation), it must ensure peers
  serving that chain retain the data and respond to sync requests with the
  `ADDITIONAL_DATA` bit set.
- If no peer has the data, a full node importing that block will receive `None` from
  `block_additional_data()` (`substrate/client/db/src/lib.rs`, Backend trait default impl
  returning `Ok(None)`). The node can then decide locally whether to reject the block or
  accept it without the data, depending on the chain's policy.
- For parachains, `validate_block` (Cumulus) will receive `None` in the
  `ParachainBlockData::V3::additional_data` field if the data was not fetched; the
  validation logic must handle this case explicitly.

This is a **chain responsibility**, not enforced generically by the mechanism.

## 2. PoV/Block Size Budget: Additional Data Counts Against Parachain PoV Limits

**Mechanism:** The additional data is packed into `ParachainBlockData::V3`
(`cumulus/primitives/core/src/parachain_block_data.rs:72-87`, new variant with
`additional_data: Vec<Option<Vec<u8>>>` field). The PoV size is computed by encoding the
entire `ParachainBlockData` structure, which includes this field.

**Parachain Constraint:** When a parachain collator builds a candidate, it collects the
additional data blob via `RecordingAdditionalDataProvider::take_data()`
(`substrate/primitives/additional-data/src/lib.rs`, todo 2 implementation) and passes it to
`ParachainBlockData::new(...)` (`cumulus/client/collator/src/service.rs:279-311`). The
resulting `ParachainBlockData::V3` is then encoded and included in the candidate's PoV. The
parachain's PoV size limit (enforced by the relay chain) applies to the entire encoded
structure, including the additional data.

**Solochain Constraint:** For non-parachain (solochain) chains, there is no formal,
generic limit on the additional data size. A solochain that opts into this feature is
responsible for enforcing its own size limits via chain-specific logic (e.g., in the
runtime's `on_finalize` hook or in the block-authoring layer). The mechanism provides no
built-in quota or gas accounting.

## 3. Pruning and Archive Interaction: Pruned With Block Body

**Mechanism:** The additional data is stored in the `ADDITIONAL_DATA` column
(`substrate/client/db/src/lib.rs:466-490`, column index 13). It is pruned alongside the
block body via the existing `prune_block` function (`substrate/client/db/src/lib.rs:2183-2237`).

**Precedent:** This mirrors the `indexed_body` mechanism (`substrate/client/db/src/lib.rs`,
column `BODY_INDEX = 12`), which stores transaction indices and is pruned with the block
body. The `prune_block` function removes entries from both `BODY` and `BODY_INDEX` columns
in the same operation; the new code adds `ADDITIONAL_DATA` to this same removal set.

**Archive Nodes:** Archive nodes (configured with `PruningMode::ArchiveAll`) retain all
blocks and thus retain all additional data. Pruned nodes (configured with
`PruningMode::Constrained`) remove additional data for blocks older than the configured
retention window, matching the block body's lifecycle.

## 4. Non-Executing Import Paths: Why Explicit Hash Verification Is Necessary

**Mechanism:** The block import pipeline has two distinct paths:

1. **Executing Path** (normal full-node import): The block is re-executed via
   `execute_block` (`substrate/client/consensus/common/src/import_queue.rs`, generic
   import queue). During execution, the runtime calls `sp_additional_data::push()` and
   `finalize()` (todo 2 host functions), which populate a `ReplayAdditionalDataProvider`
   instance. After execution, the header's digest is checked: if it contains a
   `DigestItem::AdditionalData(hash)`, the provider's computed hash must match it. This
   check is the **header-equality backstop** - if the runtime re-executed correctly, the
   hash will match.

2. **Non-Executing Paths** (skip_execution, gap sync, warp sync): These paths do NOT
   re-execute the block. They receive the block and its additional data from the network
   (or from a snapshot), but the runtime never runs. Without re-execution, there is no
   `ReplayAdditionalDataProvider` to compute a hash, and header-equality cannot be checked.

**Why Explicit Hash Verification Is Necessary:** A non-executing path must verify the
additional data's integrity before importing the block. This is done via an explicit hash
check (todo 15 implementation): the node computes `hash_blob(&additional_data_blob)`
(`substrate/primitives/additional-data/src/lib.rs`, todo 2 helper function) and compares
it to the `DigestItem::AdditionalData(hash)` value in the header. If they match, the data
is authentic; if they don't, the block is rejected before import proceeds.

**Contrast with Executing Path:** The executing path does NOT need this explicit check
because header-equality is the backstop. If the runtime re-executed correctly, the digest
hash will match the re-computed hash automatically. The explicit check is redundant on the
executing path but is the ONLY verification available on non-executing paths.

## 5. Host-Function and Runtime-Upgrade Compatibility

**Mechanism:** The `sp-additional-data` crate defines two host functions: `push()` and
`finalize()` (`substrate/primitives/additional-data/src/lib.rs`, `#[runtime_interface]`
trait `AdditionalData`). These functions look up an `AdditionalDataExt` extension in the
externalities. If the extension is not registered, both functions **panic** (not silently
no-op).

**Runtime-Upgrade Sequencing:** A runtime upgrade that starts calling `push()` or
`finalize()` will fail with a missing-host-function panic if the node's executor does not
have the `sp-additional-data` host functions registered. This is a **loud, non-silent
failure** - the node crashes or logs a panic, alerting the operator.

**Requirement:** Before a runtime upgrade that uses these host functions can activate, the
node must be upgraded to include the `sp-additional-data` host functions in its executor.
For the reference integration test (todo 8), this is done by adding `sp-additional-data`'s
`HostFunctions` to the test runtime's executor tuple (`substrate/test-utils/runtime`). For
production chains, the chain's node implementation must do the same.

**Contrast with Graceful Fallback:** Unlike `storage_proof_size` (which has a graceful
fallback constant `PROOF_RECORDING_DISABLED` in `cumulus/primitives/proof-size-hostfunction/src/lib.rs:29`),
the additional-data host functions do not gracefully degrade. This is intentional: the
additional data's hash is consensus-critical (deposited into the header), so a missing
extension must fail loudly, never silently diverge.

## 6. Malicious-Peer Handling: Tampered Data Is Caught Before Import

**Mechanism:** When a node receives a block and its additional data over the network
(via the sync protocol, todo 9), the data is unpacked into `BlockImportParams::additional_data`
(`substrate/client/consensus/common/src/block_import.rs:206-260`, field added in todo 7).
Before the block is imported, an explicit hash check (todo 15) verifies that the received
data matches the header's digest:

```
computed_hash = hash_blob(&received_additional_data_blob)
header_hash = extract_from_DigestItem::AdditionalData(header.digest)
if computed_hash != header_hash:
    reject_block_before_import()
```

**Tampered Data Scenario:** If a malicious peer sends a block with a valid header digest
but tampered additional data, the hash check will fail. The block is rejected before it
reaches the import queue, before it is stored in the database, and before it affects the
chain's state.

**Precedent:** This mirrors the existing transaction-index verification in the indexed-body
mechanism (`substrate/client/db/src/lib.rs`, `apply_indexed_body` function), which verifies
that received transaction indices match the block's actual transactions.

## 7. Header-Decode Compatibility for Un-Upgraded Nodes

**Mechanism:** The new `DigestItem::AdditionalData([u8; 32])` variant is added to the
`DigestItem` enum (`substrate/primitives/runtime/src/generic/digest.rs:75-109`) with a
discriminant index of 3 (`DigestItemType::AdditionalData = 3`, per todo 1). The enum's
`Decode` implementation (manual impl at `substrate/primitives/runtime/src/generic/digest.rs:301-316`)
uses exhaustive pattern matching on the discriminant byte.

**Un-Upgraded Node Behavior:** When an un-upgraded node (one compiled before this change)
receives a block header containing a `DigestItem` with discriminant 3, the `Decode`
implementation will encounter an unknown discriminant. The manual `Decode` impl will fail
with a decode error (the discriminant 3 is not matched by any arm in the exhaustive match).

**Consequence:** The header cannot be decoded at all. The block is rejected at the
deserialization stage, before any other processing occurs. This is a **hard incompatibility**,
not a soft one.

**Coordinated Node Upgrade Requirement:** Opting a chain into this feature is NOT a
soft/silent change. It requires a **coordinated node upgrade**: all nodes in the network
must be upgraded to a version that understands the new `DigestItem::AdditionalData` variant
before any block carrying this digest is produced. If a chain produces a block with this
digest before all nodes are upgraded, the un-upgraded nodes will reject the block at the
decode stage, splitting the network.

This is a deliberate design choice: the new digest variant is a first-class part of the
block header's structure, not a hidden or optional field. It is visible to every node that
touches the header, and every node must understand it.

## Verified Sound Because

1. **Data Availability** is a chain-level policy decision, not a generic enforcement. The
   mechanism provides the storage and sync infrastructure; the chain documents its
   guarantees.

2. **PoV/Block Size Budget** is enforced by the parachain's PoV limit (relay chain) for
   parachains, and by chain-specific logic for solochains. The additional data is part of
   the encoded `ParachainBlockData::V3`, so it counts against the PoV size automatically.

3. **Pruning and Archive Interaction** follows the existing `indexed_body` precedent,
   removing additional data alongside the block body via the same `prune_block` function.

4. **Non-Executing Import Paths** have an explicit hash check (todo 15) that verifies the
   additional data before import proceeds. The executing path relies on header-equality as
   a backstop; non-executing paths use the explicit check instead.

5. **Host-Function and Runtime-Upgrade Compatibility** is enforced by loud panics if the
   host functions are missing. A runtime upgrade that calls these functions will fail
   visibly if the node is not upgraded first, preventing silent divergence.

6. **Malicious-Peer Handling** is enforced by the explicit hash check before import. Tampered
   data is rejected before it reaches the database or affects the chain's state.

7. **Header-Decode Compatibility for Un-Upgraded Nodes** is enforced by the exhaustive
   pattern match in the `Decode` implementation. Un-upgraded nodes cannot decode headers
   with the new digest variant, requiring a coordinated node upgrade before opting in.

All seven properties are backed by concrete mechanisms in the codebase, not by assumptions
or best practices. The design is sound.
