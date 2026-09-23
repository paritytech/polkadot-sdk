# After the PoC — what's left to implement

The [[DHT_POC]] tracker covers the *minimum* a full node needs to run the v2
[[../orchestrator]] end to end: discover peers, decide storage by affinity,
propagate to the right peers, and steer connections to cover subscriptions. It is
mostly done, gated off behind `v2dht_enabled()`
(`substrate/client/network/statement/src/lib.rs:203`).

This note is the complement: everything the full v2 design
(`../../StatementStore-DHT-affinity.local.md`, tracking issue #11932) describes
that the PoC **stubs, simplifies, defers, or never attempts**. It is the roadmap
from "PoC compiles and demoes" to "v2 ships".

Two kinds of item appear below:
- **Impl** — the design is settled; only code is missing.
- **White-space ⚠️** — a design decision is still open; must be resolved before or
  during implementation. These are the risk items.

---

## A. Graduate the PoC's own stubs (closest work)

These are the pieces [[DHT_POC]] leaves as no-ops; finishing them is the tail of
the PoC itself and the prerequisite for flipping the gate.

- **Peer steering — scoring, i.e. the "brain" — Feature 7 (#11935).** The
  connect/disconnect *actuator* is done (Feature 6), but it has nothing to steer
  *by*: `on_statement_imported` (`v2dht/mod.rs:253`) and the connection-error path
  are stubs, so `peers_to_disconnect` just drops the *longest-connected* undesired
  peers as a placeholder. Impl: score peers by delivery promptness/reliability +
  connection health, feed [[../duplicate-tracking]] reputation, and evict the
  lowest-*scoring* 20%. This is what [[../peer-steering]] actually means.
  - ⚠️ [[../peer-steering]]: the scoring formula, metric weights, decay, and
    disconnect cadence are intentionally unspecified.
- **v2-affinity initial sync — Feature 5 (#11934).** `on_initial_sync`
  (`v2dht/mod.rs:291`) is a stub; sync still uses the v1 `topic_affinity` filter.
  Impl: drive [[../node-join-sync]] from the peer's advertised
  [[../bloom-filter-affinity]] and its DHT closeness. Also the hook that carries
  statement **migration** as nodes join/leave — central to [[../rollout-plan]].
- **Flip the gate & run e2e ([[../rollout-plan]]).** Enable
  `STATEMENT_STORE_V2_DHT_ENABLED=1` and exercise the four flows together; today
  each is only tested in isolation.

## B. Bound & maintain the peer topology (#11933, #12272)

The PoC builds an event-fed topology but leaves it unbounded and unmanaged.

- **Known-set TTL eviction.** `discovered` grows unbounded (its own TODO at
  `v2dht/peers_topology.rs:65`). Impl [[../peer-set-eviction]]: TTL expiry, refresh
  on re-observation (discovery/identify/substream-open), exempt connected peers.
  [[../discovery-coverage]] shows discovery ≠ freshness — dead nodes never leave.
- **Connected-set cap.** The active set is uncapped. Impl a configurable cap
  ([[../peer-sets]], [[../peers-topology]]). (Configured in IN OUT peers, aks for directions where it is.)
- **Connection model — reserved vs random slots ([[../peer-connection-model]]).**
  Impl: change the notification-protocol config so the statement substream accepts
  inbound connections independently of sync; add a reserved-slot mechanism for
  DHT-closest peers.
  - ⚠️ Unspecified: reserved-vs-random slot counts and how they scale; admission
    policy when random slots are full (reject / evict random / prefer DHT-closer);
    whether a sync-protocol connection grants priority.

## C. Real DHT correctness & routing table (#11933) ⚠️ mostly white-space

The PoC approximates the DHT from connected peers and piggybacked discovery. The
design wants a proper Kademlia view — several open decisions block this.

- ⚠️ **Peer-key hash mismatch ([[../xor-keyspace]]).** Local affinity hashes peer
  ids with blake2-256, but the network DHT (libp2p/litep2p) keys on SHA-256.
  Internally consistent but *diverges from the real keyspace*, so local closeness
  ≠ true Kademlia responsibility. Must be reconciled.
- ⚠️ **Routing target: connected vs known ([[../dht-affinity-routing]]).** Routing
  targets only the closest *connected* peers; if the node isn't already connected
  to closer peers, propagation may not converge toward the true storers. Decide
  whether to target closest *known* peers (and connect on demand).
- ⚠️ **Remote DHT eligibility ([[../dht-affinity-storing]]).** Whether a remote
  peer is a full (storing) node vs light node isn't derivable from
  discovery/identify. Until known, only connected peers feed the closest-set,
  weakening storing decisions over the wider table.
- **Own routing table / dedicated DHT ([[../routing-table]],
  [[../discovery-piggyback]], [[../discovery-coverage]]).** Today discovery rides
  the chain's genesis-scoped `/kad`, binding membership to one chain (people-chain).
  ⚠️ Open participation needs a statement-store-scoped DHT (e.g.
  `/statement-store/kad`) independent of chain membership — unbuilt, deferred.

## D. Storage subsystem (#10910, #11936)

- **Per-reason store limits ([[../store-limits]]).** Impl configurable caps per
  affinity reason (DHT vs explicit).
  - ⚠️ Eviction policy when a bucket is full is unspecified: which statement drops,
    interaction with statement expiry, and what happens to DHT-affinity coverage.
- **Index-locking refactor ([[../index-locking]]).** Stage 1: split
  `WriteIndex`/`ReadIndex` under independent locks. Stage 2: disk-back the read
  index with a bounded LRU; replace the full accounts map with a summary + LRU.
- **Replace the transient-store hack.** The PoC holds non-persistent statements in
  `query_index.transient` and serves each once — its TODO marks this a temporary
  #11932 mechanism. Needs a real retention/expiry design.

## E. Privacy hardening (#11934)

- **Correlation mitigation ([[../privacy-correlation-risk]]).** Filter *exchange*
  is done (Feature 4), but the anti-correlation strategy isn't: connect to only a
  *subset* of a topic's k-closest peers and tune to a *narrower* bloom filter when
  connecting inside a topic neighbourhood (delivery-vs-privacy trade-off).

## F. Propagation optimisations (deferred)

Core forwarding is PoC-complete ([[../forwarding-decision]],
[[../multi-topic-replication]], [[../periodic-propagation]],
[[../propagation-termination]], [[../duplicate-tracking]]). What remains is
explicitly future work:

- **Duplicate-statement problem ([[../duplicate-statement-problem]]).** Overlapping
  peers deliver the same statement up to P times; unmitigated.
- ⚠️ **Set reconciliation ([[../set-reconciliation]]).** Keep 1–2 live forwarders +
  periodically reconcile with others to cut duplicates. Mechanism deferred entirely.

## G. Light node — the design's stated priority, still untouched (#11934)

The design's §5 Implementation Plan puts **light node support first** (Stages 1–2,
before full-node DHT), because v1's bandwidth already limits it to full nodes. The
PoC went full-node-first, so this whole track is essentially unstarted.

- Extend smoldot to speak the statement wire protocol; basic dedup
  ([[../light-node-protocol]]).
- Bloom filtering for light nodes (handshake + updates; discard false positives).
- Light-node DHT affinity: build its own routing table, discover DHT/explicit-affine
  peers, connect to multiple full nodes, run [[../peer-steering]].

> Note the ordering tension: the design sequences light-node → full-node DHT, but
> the PoC delivered full-node DHT first. Worth an explicit decision on which track
> leads post-PoC.

## H. Orchestration & rollout (#11937, #11938)

- **Complete the [[../orchestrator]] (#11937).** Fully wire the remaining decision
  points (`on_peer_connected`, `on_validate_inbound_substream`, `on_major_sync_end`
  are stubs kept as hooks), connection-acceptance policy, and statement migration.
- ⚠️ **Rollout & migration ([[../rollout-plan]], #11938).** No migration path for
  existing stored statements and no coordination mechanism (version gating / flag
  day / dual-protocol window) is specified. Clarify v1-fallback: is it purely
  transitional for the migration window, and when is it removed (no v1 interop)?
- **Production config from simulation ([[../simulation]]).** Pin K (replication
  factor) and gossip-target counts from simulation results before production.

---

## PoC feature → remaining tail (quick map)

| PoC feature ([[DHT_POC]]) | Status | Remaining after PoC |
| --- | --- | --- |
| 1 Discovery | ✅ | Known-set TTL, connected cap (B); real routing table / dedicated DHT (C) |
| 2 Receive & store | ✅ | Per-reason limits + eviction, index-locking, transient replacement (D) |
| 3 Propagate | ✅ | Connected-vs-known routing (C); dup/reconciliation optimisations (F) |
| 4 Filter exchange | ✅ | Correlation mitigation (E) |
| 5 Initial sync | ❌ stub | v2-affinity sync + migration (A) |
| 6 Peer steering — actuator | ✅ | Only the connect/disconnect plumbing; today it drops the *longest-connected* undesired peers, not the least useful ones |
| 7 Peer steering — scoring (the "brain") | ❌ stub | The reputation/scoring that decides *whom* to keep — without it, steering has no intelligence. Signals + formula (A) ⚠️ |
| 8 Subscription coverage | ✅ | Coverage target is stable; *which* peers get evicted stays arbitrary until scoring (7) lands |
| — | — | Light node track, entirely (G); orchestrator + rollout (H) |

## Reading order

Nearest-term: **A** (finish stubs, flip gate) → **B/D** (bound topology, storage) →
**C** (DHT correctness white-spaces) → **H** (rollout). **G** (light node) is a
parallel track and the design's nominal priority. **E/F** are hardening and
optimisation.
