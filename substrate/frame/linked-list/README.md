# Linked-list pallet

A generic per-list sorted doubly-linked list. Items live in independent lists
keyed by `ListId`; within a list they are kept in strict priority order — head
is highest priority, tail is lowest. Same-priority items land on the tail side
of their cluster, so tail-first iteration is LIFO within a priority cluster.

## Overview

Consumer pallets use the [`SortedListInterface`] trait — `insert`, `remove`,
`re_insert`, `pop_tail`, `iter_from_tail`, and a handful of read helpers. A
single permissionless dispatchable, `reprioritize`, refreshes an item's stored
priority against the [`PriorityProvider`] when the consumer's authoritative
priority has drifted.

Inserts and re-inserts take a [`Position`] hint (a typed `(prev, next)` pair
where endpoints are encoded as `None`) and run in O(1) when the hint is valid.
When the hint is stale, the pallet **repairs it on-chain** up to a configurable
budget. This is the central design choice and the reason most of the
complexity in `list.rs` exists.

## Why we have a hint-repair flow

Storage-backed sorted lists are stuck between two undesirable extremes:

**A pure on-chain search is too expensive.** Computing the correct insertion
site from scratch costs O(N) storage reads. With per-block weight budgets,
that is incompatible with hot consumer paths.

**A pure off-chain hint is too brittle.** The cheap alternative is to find the
position off-chain via the `find_position` view helper (also O(N), but free of
on-chain weight) and pass the result as a hint. The problem is the gap between
computing the hint and dispatching the extrinsic: other transactions in the
meantime can insert, remove, or reprioritize items around the hinted position.
A naive "reject if stale" policy forces callers to recompute and retry.

**Bounded on-chain repair is the middle path.** When the supplied hint does
not match the current list state, the pallet walks at most
`MaxHintRepairSteps` nodes from the hint toward the correct position. The
dispatch reports the actual number of steps walked back via
`PostDispatchInfo::actual_weight`, so callers are refunded unused weight.
Repair handles three flavours of staleness, each counted as one step:

- **Dangling references** — a hinted neighbor has been removed since the hint
  was prepared. The repair clears the affected side of the cursor and
  re-evaluates.
- **Link inconsistency** — a new item was spliced between the hinted `prev` and
  `next`, so they are no longer adjacent. The repair re-anchors the cursor on
  whichever cached link still admits the priority (preferring the `prev` side
  on ties).
- **Priority drift** — the hinted region's priority bracket no longer contains
  the new item. The repair steps one node head- or tail-ward and tries again.

If the budget is exhausted before a valid position is reached, the dispatch
fails with `InvalidPositionHints` rather than corrupting the list. Callers can
recompute and retry — but in practice the budget is sized so retry is
exceptional, not the common case.

### Picking `MaxHintRepairSteps`

- **Too tight** — mild contention causes spurious `InvalidPositionHints`
  failures; callers retry-spam.
- **Too generous** — the pre-dispatch weight reservation is large even when
  most calls walk zero steps, reducing parallel transactions per block.

A reasonable default targets "the worst-case drift expected within one block of
the hint being prepared, plus a small margin". Consumer pallets should pick the
value by simulating expected contention against their list cardinality.

## Storage layout

- `ListNodes[(list_id, item)] -> Node { prev, next, priority }` — the per-item
  record. `prev`/`next` are `None` at endpoints; `priority` is cached so
  position checks do not need to consult the consumer's source of truth.
- `ListHeads[list_id] -> ItemId` — the highest-priority item per list.
- `ListTails[list_id] -> ItemId` — the lowest-priority item per list.
- `ListSizes[list_id] -> u32` — the per-list node count (the row is removed
  when the list empties).

## Public surface

Consumer pallets use [`SortedListInterface`]:

- **`insert`** — places `item` at `priority` using a hint. O(1) on a valid
  hint; bounded otherwise.
- **`remove`** — splices `item` out. O(1).
- **`pop_tail`** — removes and returns the lowest-priority item. O(1). This is
  the LIFO primitive for tail-side consumers.
- **`re_insert`** — updates an item's priority. The fast path mutates in place
  when the existing neighbors still admit the new priority; otherwise it
  splices out and re-inserts at the supplied hint inside a transactional layer
  so a budget-exhaustion failure rolls back cleanly.
- **`iter_from_tail`** — bounded tail-first iteration.
- Read helpers: `head`, `tail`, `count`, `contains`, `neighbors`, `priority`,
  `find_position`, `find_re_insert_position`, `repair_steps_needed`.

The single dispatchable, `reprioritize`, is permissionless: anyone can call it
to refresh `(list_id, item)`'s priority from [`PriorityProvider`] when it has
drifted from the stored value. This is the mechanism by which consumer pallets
surface authoritative priority changes (e.g. collateral ratio shifts) to the
list.

## Try-state invariants

`try_state` (active under `try-runtime`) checks, for each list:

1. Every present node implies `ListSizes[list_id] >= 1`.
2. `ListHeads`/`ListTails` agree with the node graph at both endpoints.
3. The chain from head to tail visits every node exactly once.
4. Priorities are non-increasing from head to tail (`>=` allows same-priority
   clusters).
5. No orphan nodes — every `ListNodes` row is reachable from `ListHeads`.

See `try_state.rs` for the exact checks.

## Weights and refunds

The `reprioritize` dispatchable is weighted by `MaxHintRepairSteps` upfront and
refunds the difference between the budget and steps actually walked.
Trait-level `insert` and `re_insert` likewise return the step count so consumer
pallets can do the same accounting at their own dispatchable boundary.
