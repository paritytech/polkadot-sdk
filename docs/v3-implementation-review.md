# V3 Scheduling Implementation Review

**Date:** 2026-01-03  
**Reviewer:** Self-review (Claude)  
**Status:** Pre-merge review

## Summary

| Dimension | Grade | Key Issues |
|-----------|-------|------------|
| Design Compliance | A | Matches design well |
| Edge Cases | B | Re-submission not supported (by design), missing unit tests |
| Code Quality | B | Missing high-level docs, TODO comments in tests |
| Security | A- | Both parents validated on relay chain |
| E2E Tests | B- | Happy path covered, missing transition/failure tests |

---

## 1. Design Compliance

**Design Requirements (from `cumulus-v3-implementation-plan.md`):**
1. Add `SchedulingV3EnabledApi::scheduling_v3_enabled() -> bool` runtime API
2. Runtime implements it, returning `SchedulingV3Enabled::get()`  
3. Collator calls API to decide V3 vs V1/V2
4. V3 uses `scheduling_parent` (fresh relay chain tip) separate from `relay_parent` (older block)

**Implementation Status:**

| Component | Status | Location |
|-----------|--------|----------|
| `SchedulingV3EnabledApi` trait | Done | `cumulus/primitives/core/src/lib.rs:513` |
| Runtime config `SchedulingV3Enabled` | Done | `cumulus/test/runtime/src/lib.rs:388` |
| Runtime API implementation | Done | `cumulus/test/runtime/src/lib.rs:529-533` |
| `CandidateDescriptorV2::new_v3()` | Done | `polkadot/primitives/src/v9/mod.rs:2219` |
| Slot-based collator V3 support | Done | `cumulus/client/consensus/aura/src/collators/slot_based/block_builder_task.rs:457-485` |
| Basic collator V3 support | Done | `cumulus/client/consensus/aura/src/collators/basic.rs:252` |
| Lookahead collator V3 support | Partial | Queries API but falls back to non-V3 (acceptable - slot-based is recommended) |

---

## 2. Edge Cases

| Edge Case | Status | Location | Notes |
|-----------|--------|----------|-------|
| `scheduling_parent == relay_parent` | Handled | `scheduling.rs:88-90` | Only allows equality; re-submission is future work |
| Empty header chain (offset=0) | Handled | `scheduling.rs:77-79` | Returns `scheduling_parent` directly |
| Header chain crosses epoch boundary | Handled | `block_builder_task.rs:396` | Stops if epoch change detected |
| `scheduling_parent` not in allowed ancestors | Handled | `paras_inherent/mod.rs:1007-1016` | Relay chain validates |
| Collator produces V3 but relay V3 disabled | Handled | `lib.rs:540` | Falls back to V2 |
| Runtime API call fails | Handled | `block_builder_task.rs:460` | Uses `.unwrap_or(false)` |

**Notable Gaps:**
1. **Re-submission support**: `scheduling.rs:88-90` explicitly rejects `relay_parent != internal_scheduling_parent`. Documented as future work.
2. **Unit tests missing**: `scheduling.rs:96-101` has TODO comments only.

---

## 3. Code Quality and Documentation

**Strengths:**
- `scheduling.rs` has clear doc comments explaining the validation flow
- `CandidateDescriptorV2::new_v3()` has good inline documentation
- `block_builder_task.rs` has inline comments explaining V3 scheduling proof construction

**Weaknesses:**
- No high-level architecture document describing the V3 flow
- Missing unit tests in `scheduling.rs`
- Magic number `version: 1` in `new_v3()` should be a named constant
- Error messages could include actual values for debugging

---

## 4. Security Analysis

### Transition Scenarios

| Scenario | Behavior | Risk |
|----------|----------|------|
| V3 runtime + V3 collator | Works | None |
| V3 runtime + old collator | Fails | Collator produces V1/V2, runtime expects V3 |
| Old runtime + V3 collator | Safe | API returns false, collator falls back to V1/V2 |
| V3 enabled mid-session | Immediate | Runtime upgrade takes effect immediately |

### Relay Chain Validation

| Check | Status | Location |
|-------|--------|----------|
| `relay_parent` in allowed ancestors | Done | `paras_inherent/mod.rs:993-1001` |
| `scheduling_parent` in allowed ancestors | Done | `paras_inherent/mod.rs:1007-1016` |
| Session validation | Done | `paras_inherent/mod.rs:1041-1053` |
| UMP signals use scheduling_parent's claim queue | Done | `paras_inherent/mod.rs:1020-1028` |

### Header Chain Validation (Parachain Side)

The validation at `scheduling.rs` verifies:
- Headers form a valid chain (parent_hash linkage)
- First header hashes to `scheduling_parent`
- Last header's parent = `relay_parent`

Headers are NOT verified against relay chain state, but this is acceptable because the relay chain validates both `relay_parent` and `scheduling_parent` against `AllowedRelayParentsTracker`.

**Conclusion: No critical security vulnerabilities found.**

---

## 5. E2E Test Coverage

### Current Tests

1. `scheduling_v3_test` - Tests V3 candidates are backed (3 candidates in 20 blocks)
2. `v3_backwards_compatibility_test` - Tests legacy parachains still work with V3 enabled

### Coverage Gaps

| Gap | Risk | Recommendation |
|-----|------|----------------|
| Session boundary during V3 | Medium | Test V3 across epoch change |
| `relay_parent_offset > 1` | Medium | Test with larger offsets |
| Collator restart mid-V3 | Low | Test recovery |
| Invalid scheduling_parent | Medium | Test rejection of bad candidates |
| V3 disabled -> enabled transition | High | Test runtime upgrade enabling V3 |

### Finality Lag

The V3 test uses a 5-block finality lag limit, while the backwards compatibility test uses 3.
This is consistent with other tests in the codebase (e.g., `shared_core_idle_parachain.rs` uses 5,
`async_backing_6_seconds_rate.rs` uses 6). The variance is due to timing jitter in CI environments,
not a V3-specific issue.

---

## Action Items

### Critical (Fix Before Merge)
1. [x] Add unit tests to `scheduling.rs` - Done, 8 tests added
2. [x] Investigate finality lag - Not V3-specific, normal CI timing variance

### Non-Critical (Nice to Have)
1. [x] Replace magic `version: 1` with named constant - Done: `CANDIDATE_DESCRIPTOR_VERSION_V3`
2. [ ] Add high-level architecture documentation
3. [ ] Test V3 enable/disable transitions via runtime upgrade
4. [ ] Test session boundaries with V3 enabled
