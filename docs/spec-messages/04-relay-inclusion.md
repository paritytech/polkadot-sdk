# Speculative Messaging: Relay Chain Inclusion & Verification

**Location:** `polkadot/runtime/parachains/src/inclusion/mod.rs`

The relay chain's role is minimal: it stores the most recent `provides` root per parachain and verifies that each `requires` commitment references a valid provides root. It never sees message contents or verifies MMR proofs — that work happens in the PVF.

## CandidateCommitments Extension

**File:** `polkadot/primitives/src/v9/mod.rs`

Two new fields added to `CandidateCommitments<N>`:

```rust
pub struct CandidateCommitments<N> {
    // ... existing fields (upward_messages, horizontal_messages, etc.) ...

    /// Speculative messaging provides commitment.
    /// Top-level Merkle root over all per-destination MMR roots.
    pub provides: Option<ProvidesCommitment>,

    /// Speculative messaging requires commitments.
    /// Each entry references a source para's provides root.
    pub requires: BoundedVec<RequiresCommitment, ConstU32<MAX_REQUIRES_COMMITMENT_NUM>>,
}
```

## ValidationResult Extension

**File:** `polkadot/parachain/src/primitives.rs`

Matching fields in `ValidationResult` (returned by the PVF):

```rust
pub struct ValidationResult {
    // ... existing fields ...

    /// Speculative messaging provides root (Option<H256>)
    pub provides_spec_msg_root: Option<sp_core::H256>,

    /// Speculative messaging requires: Vec of (source_para_id, expected_provides_root)
    pub requires_spec_msg: BoundedVec<(Id, sp_core::H256), ConstU32<MAX_REQUIRES_COMMITMENT_NUM>>,
}
```

**Bound constant:**

```rust
/// Maximum number of requires commitments allowed per candidate.
/// Limits relay chain storage reads during acceptance check.
pub const MAX_REQUIRES_COMMITMENT_NUM: u32 = 1024;
```

---

## Relay Chain Storage

### IncludedProvidesRoots

```rust
#[pallet::storage]
pub(crate) type IncludedProvidesRoots<T: Config> = StorageMap<
    _,
    Twox64Concat,
    ParaId,
    ProvidesCommitment,
>;
```

Stores the most recently included provides root per parachain. Updated when a candidate with a `provides` commitment is enacted.

---

## Verification Flow

### 1. Candidate Acceptance Check (check_validation_outputs)

When a candidate is being validated for inclusion:

```
for each req in candidate.commitments.requires:
    source_provides = IncludedProvidesRoots::get(req.source)

    if source_provides is None:
        REJECT: SpeculativeMessagingMismatch
        (source para has never published a provides root)

    if req.expected_root != source_provides.root:
        REJECT: SpeculativeMessagingMismatch
        (root mismatch — stale or incorrect reference)

    if req.source != source:
        REJECT: SpeculativeMessagingMismatch
        (source ParaId doesn't match)

Also enforce:
    requires.len() <= MAX_REQUIRES_COMMITMENT_NUM
    Otherwise: REJECT: TooManyRequiresCommitments
```

### 2. Candidate Enactment (enact_included_candidate)

When a candidate is successfully included:

```
if let Some(provides) = &commitments.provides {
    IncludedProvidesRoots::insert(para_id, provides);
}
```

Only the **most recent** provides root is kept. Previous roots are overwritten.

### 3. Parachain Offboarding Cleanup

```rust
pub(crate) fn cleanup_outgoing_provides_roots(outgoing: &[ParaId]) {
    for outgoing_para in outgoing {
        IncludedProvidesRoots::remove(outgoing_para);
    }
}
```

Called from `finish_candidate_session`. Prevents stale provides roots from persisting after a parachain deregisters, which could cause incorrect matches if the `ParaId` is later reused.

---

## Error Types

```rust
pub enum AcceptanceCheckErr {
    // ... existing errors ...
    SpeculativeMessagingMismatch,    // requires root doesn't match source's provides
    TooManyRequiresCommitments,      // exceeds MAX_REQUIRES_COMMITMENT_NUM
}
```

---

## Diagram: Relay Chain Verification

```
         ParaA candidate                     ParaB candidate
         ================                    ================
         provides: Some(root_A)              requires: [{source: ParaA, root: root_A}]

                |                                    |
                v                                    v
    IncludedProvidesRoots                  check_validation_outputs
    [ParaA] = root_A                         |
         (on enactment)                      v
                                    IncludedProvidesRoots::get(ParaA)
                                         |
                                         v
                                    root_A == root_A ?
                                    YES -> accept
                                    NO  -> SpeculativeMessagingMismatch
```

## Timing Considerations

```
Relay Block N:   ParaA candidate included with provides: root_A_v1
                 IncludedProvidesRoots[ParaA] = root_A_v1

Relay Block N+1: ParaA produces new block with more messages
                 ParaA candidate included with provides: root_A_v2
                 IncludedProvidesRoots[ParaA] = root_A_v2  (overwritten!)

                 ParaB built against root_A_v1 (stale!)
                 ParaB requires: [{source: ParaA, root: root_A_v1}]
                 Check: root_A_v1 != root_A_v2 -> MISMATCH

                 BUT: ParaB includes a LateBlockProof in its PoV
                 PVF transforms: root_A_v1 -> root_A_v2
                 ParaB requires (after PVF): [{source: ParaA, root: root_A_v2}]
                 Check: root_A_v2 == root_A_v2 -> ACCEPTED
```

This is why late block proofs exist — they handle the race condition where a source publishes new messages between when the receiver builds its block and when the relay chain checks the commitments.

---

## Key Design Decisions

1. **Relay chain does NO proof verification** — all cryptographic work (MMR proofs, Merkle proofs, extension proofs) happens in the PVF. The relay chain only compares hashes.

2. **Only most recent root stored** — `IncludedProvidesRoots` keeps one root per para, not a history. Late block proofs handle timing mismatches.

3. **Bounded requires** — `MAX_REQUIRES_COMMITMENT_NUM = 1024` prevents a malicious parachain from causing unbounded storage reads on the relay chain.

4. **HRMP untouched** — Speculative messaging runs alongside HRMP. No HRMP code is modified; both systems can coexist.

5. **Cleanup on offboarding** — Stale provides roots removed when a para deregisters, preventing `ParaId` reuse attacks.
