# DHT-affinity PoC — implementation tracker

The minimum a node must do to run the v2 [[../orchestrator]] end to end: discover
peers, decide storage by affinity, propagate to the right peers, and steer
connections so every RPC subscription is covered. This note tracks the PoC
against the code, marking each piece ✅ done, ⚠️ partial, or ❌ not started.

Status legend: ✅ implemented · ⚠️ partial · ❌ not started.

> **Everything here is gated.** `v2dht_enabled()` (`substrate/client/network/statement/src/lib.rs:203`) reads the
> `STATEMENT_STORE_V2_DHT_ENABLED` env var and defaults `false`, so the v2 path is
> off (but opt-in-toggleable, per the CLI flags in #12456) unless explicitly
> enabled ([[../v2-overview]]). The `ExplicitAffinity` and `PeersTopology` modules are
> near-complete; the gaps are in the [[../orchestrator]] wiring and the store.
> Turning the gate on end to end is the last PoC step, not the first.

> **All PoC pieces are now merged into the feature branch.** Features 2, 6 and 8 —
> formerly on branches `ae-store-received-category` and `ae-peer-steering` — landed
> via #12390/#12515 (retention) and #12393 (peer steering); the
> `V2DhtOrchestrator::new` reconciliation those branches implied is done.

## The four flows the PoC must support

1. **Network discovery** — feed [[../peers-topology]] most of the network's peers.
2. **Receiving a statement** — store it when `dht.is_affine(stmt)` or
   `topics.is_affine(stmt)` holds ([[../statement-storage]]), else keep it only
   until the next propagation.
3. **Propagating a statement** — for each topic, send to `routing_targets` plus
   the connected peers matched by `peer_has_explicit_affinity`, batched per peer
   ([[../forwarding-decision]], [[../multi-topic-replication]]).
4. **Make subscribe work** — a subscription on any node receives every matching
   statement in the network, achieved by steering connections to cover the
   node's topics ([[../client-rpc]], [[../peer-steering]]).

---

## Feature 1 — Network discovery ✅

The [[../orchestrator]] feeds [[../peers-topology]] from network events:
`on_peers_discovered` (dispatch `substrate/client/network/statement/src/lib.rs:934`, def `substrate/client/network/statement/src/v2dht/peers_topology.rs:93`) and
`on_peer_identified` (dispatch `substrate/client/network/statement/src/lib.rs:937`, def `substrate/client/network/statement/src/v2dht/peers_topology.rs:103`)
build the XOR-ordered peer index; substream open/close track the connected set.
`PeersTopology` exposes `routing_targets`, `peers_for_topics`, `connected_peers`
(`substrate/client/network/statement/src/v2dht/peers_topology.rs`).

- ❌ **No eviction.** `discovered` grows unbounded; the module's own TODO
  (`substrate/client/network/statement/src/v2dht/peers_topology.rs:65`) flags it. See [[../peer-set-eviction]].

## Feature 2 — Receive & store with the right obligation ✅ (#12390, #12515)

Landed under different names than the task list: a `RetentionReasonMask` bitmask
(`substrate/primitives/statement-store/src/store_api.rs`), not a `StorageObligation` enum.

- ✅ **Retention mask + transient submit.** `RetentionReasonMask` carries
  `TRANSIENT` (empty), `DHT_AFFINITY`, `EXPLICIT_AFFINITY`; `is_persistent()` is
  "any bit set". `StatementStore::submit_with_retention_mask` is the new entry
  point; `submit` now means `persistent()`.
- ✅ **Held until next propagation, then dropped.** The store keeps transient
  statements in `query_index.transient` (`substrate/client/statement-store/src/lib.rs`),
  never writes them to the DB, hides them from the query API, and serves each
  once via `take_recent_statements` before dropping it. (Its TODO marks this a
  temporary PoC mechanism for #11932.)
- ✅ **Receive path chooses the obligation.** `AffinityView::resolver`
  (`substrate/client/network/statement/src/v2dht/mod.rs:61`) returns a closure the store calls per statement: it sets
  `DHT_AFFINITY` when `dht.is_affine(stmt)` and `EXPLICIT_AFFINITY` when
  `topics.is_affine(stmt)`, falling back to `persistent()` if a lock is poisoned.
  `substrate/client/network/statement/src/lib.rs` threads the mask through the submit queue to
  `submit_with_retention_mask`. See [[../statement-storage]], [[../store-limits]].

## Feature 3 — Propagate to the right peers ✅

`V2DhtOrchestrator::propagation_plan` (`substrate/client/network/statement/src/v2dht/mod.rs:265`) builds, per connected
peer, the indices of statements it should receive: the union of `routing_targets`
over each topic plus the connected peers matched by `peer_has_explicit_affinity`,
deduplicated so a peer gets a statement once. Wired into `propagate_statements`
(`substrate/client/network/statement/src/lib.rs:1640`), sent batched via `send_targeted_statements_to_peer`
(`substrate/client/network/statement/src/lib.rs:1577`), tested.

- Routing uses connected-peer XOR distance only; the remote-DHT-eligibility
  white-space stands ([[../dht-affinity-routing]]).

## Feature 4 — Wire the affinity filter exchange ✅ (#12373)

`StatementMessage::ExplicitTopicAffinity(AffinityFilter)` (`substrate/client/network/statement/src/lib.rs:143`) carries
the bloom filter ([[../bloom-filter-affinity]]). Published on change via
`broadcast_local_filter` (`substrate/client/network/statement/src/lib.rs:996`) and to each newly connected peer via
`send_local_filter` (`substrate/client/network/statement/src/lib.rs:1010`). Incoming filters route to
`on_peer_filter_update` (dispatch `substrate/client/network/statement/src/lib.rs:1341`, def `substrate/client/network/statement/src/v2dht/mod.rs:240`) and drop
on disconnect via `on_peer_disconnected` (dispatch `substrate/client/network/statement/src/lib.rs:1160`, def
`substrate/client/network/statement/src/v2dht/mod.rs:213`). `AffinityFilter` (`substrate/client/network/statement/src/affinity.rs`) has a snapshot-pinned wire
format.

## Feature 5 — Serve stored statements to a newly connected peer ⚠️

The v1 mechanism exists: `schedule_initial_sync_for_peer` (`substrate/client/network/statement/src/lib.rs:1665`) queues a
peer and `process_initial_sync_burst` (`substrate/client/network/statement/src/lib.rs:1724`) sends its matching
statements round-robin, filtering by the peer's v1 `topic_affinity`. The store
already hands a fresh RPC subscription its existing matches before streaming new
ones.

- ❌ **Not driven by v2 affinity.** `V2DhtOrchestrator::on_initial_sync` is a
  stub (`substrate/client/network/statement/src/v2dht/mod.rs:291`); the filtered sync does not yet use the peer's
  advertised `AffinityFilter` or its DHT closeness ([[../node-join-sync]]).

## Feature 6 — PeerSteering module + connection actuator ✅ (#12393)

New `substrate/client/network/statement/src/v2dht/peer_steering.rs`. `PeerSteering` keeps its own connected set (fed by
substream events) and a desired set (supplied by the orchestrator).
`peers_to_connect` / `peers_to_disconnect` reconcile the two, closing at most
`MAX_DISCONNECT_PERCENT` (20%, `substrate/client/network/statement/src/v2dht/peer_steering.rs:47`) of connections per refresh so
the set converges over several refreshes. `refresh_connections` applies the
decision through the statement protocol's reserved set
(`add_/remove_peers_*_reserved_set`). See [[../peer-steering]],
[[../peer-connection-model]].

## Feature 7 — Score peers from statements & connection problems ❌

No scoring. `peers_to_disconnect` drops the lowest peer ids as a placeholder
("No score yet to rank by"); `on_statement_imported` is still a stub
(`substrate/client/network/statement/src/v2dht/mod.rs:253`) and the connection-error path feeds nothing. Required before
`refresh_connections` can drop the *lowest-scoring* 20% rather than arbitrary
peers ([[../peer-steering]] open question: the scoring formula is unspecified).

## Feature 8 — Steer connections to cover subscriptions ✅ (#12393, #12488)

The affinity-tick chain is wired: `on_pending_affinities` (`substrate/client/network/statement/src/v2dht/mod.rs:297`)
does `topics()` → `peers_for_topics(...)` → `update_peers_needing_connections`,
and `refresh_connections(&network)` (`substrate/client/network/statement/src/v2dht/mod.rs:305`) runs on the same tick
(`substrate/client/network/statement/src/lib.rs:904`–`905`).

- ✅ **Coverage target is now connection-independent** (#12488).
  `peers_for_topics` (`substrate/client/network/statement/src/v2dht/peers_topology.rs:200`) selects a minimal set of the
  closest known peers covering every topic, "independent of connection state", so
  a peer already covering its topic is no longer dropped and re-added. The test
  `peers_for_topics_keeps_connected_coverage_peer` (`substrate/client/network/statement/src/v2dht/peers_topology.rs:501`) locks
  this in. The earlier flapping-target white-space is resolved.
- ⚠️ Eviction quality still depends on Feature 7: `refresh_connections` drops
  arbitrary (lowest-id) peers rather than lowest-scoring ones until scoring lands.

---

## Orchestrator stubs still blocking the PoC

`V2DhtOrchestrator` (`substrate/client/network/statement/src/v2dht/mod.rs`) is wired into the event loop at ~15 call
sites; these bodies remain no-ops:

- `on_initial_sync` (`substrate/client/network/statement/src/v2dht/mod.rs:291`) — Feature 5.
- `on_statement_imported` (`substrate/client/network/statement/src/v2dht/mod.rs:253`) — Feature 7 (scoring).
- `on_peer_connected` (`substrate/client/network/statement/src/v2dht/mod.rs:208`), `on_validate_inbound_substream`
  (`substrate/client/network/statement/src/v2dht/mod.rs:219`), `on_major_sync_end` (`substrate/client/network/statement/src/v2dht/mod.rs:309`) — kept as decision points;
  may be removed.

(`on_pending_affinities` and `refresh_connections` are implemented — Feature 8.)

## Suggested PoC order

1. ~~Merge Features 2, 6, 8 into the feature branch.~~ Done (#12390/#12515, #12393).
2. ~~Replace the flapping coverage target (Feature 8).~~ Done (#12488).
3. v2-affinity initial sync — implement `on_initial_sync` (Feature 5).
4. Peer scoring — `on_statement_imported` + connection-error path (Feature 7).
5. Enable `v2dht_enabled()` (`STATEMENT_STORE_V2_DHT_ENABLED=1`) and run the four
   flows end to end ([[../rollout-plan]]).

Done already: filter exchange (4), targeted propagation (3), retention mask (2),
and peer steering + subscription coverage (6, 8) — all merged. Remaining PoC work
is initial sync (5) and peer scoring (7).
