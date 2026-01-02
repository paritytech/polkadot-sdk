# Cumulus V3 Implementation Plan

## Collator/Omninode Integration (TODO)

The collator needs to know whether to produce V3 or V1/V2 candidates. This mirrors how
RelayParentOffsetApi works today:

### Current Pattern (RelayParentOffset)
1. Runtime implements RelayParentOffsetApi::relay_parent_offset() -> u32
2. Collator calls this API to get the offset value
3. Collator uses the offset when building candidates

### Required Pattern (V3)
1. Add new runtime API: SchedulingV3EnabledApi::scheduling_v3_enabled() -> bool
2. Runtime implements it, returning SchedulingV3Enabled::get()
3. Collator calls this API to decide:
   - If false: produce V1/V2 candidates with relay_parent_descendants in inherent
   - If true: produce V3 candidates with header_chain in PVF extension

### Files to modify:
- cumulus/primitives/core/src/lib.rs - Add SchedulingV3EnabledApi trait
- cumulus/client/consensus/aura/src/collators/slot_based/ - Query API, build V3 candidates
- cumulus/polkadot-omni-node/lib/src/common/mod.rs - Add API to requirements
- All runtime impl_runtime_apis! blocks - Implement the new API

### Upgrade Path for Parachain Teams:
1. Update collator nodes to version supporting V3
2. Runtime upgrade: set SchedulingV3Enabled = ConstBool<true> and implement API
3. Collators automatically switch to V3 candidate production

This ensures a safe upgrade path where:
- Old collators with new runtime: will fail (runtime expects V3 but collator sends V1/V2)
- New collators with old runtime: will work (API returns false, collator sends V1/V2)
- New collators with new runtime: V3 works
