# Statement Store v2 — design notes

Zettelkasten-style notes on the v2 (DHT-affinity) statement-store protocol.
Source design: `../StatementStore-DHT-affinity.local.md`. Tracking issue:
paritytech/polkadot-sdk#11932.

Each note is atomic (one idea) and links to others with `[[slug]]`. Filenames
are the stable note IDs. Notes flagged ⚠️ carry an "Open questions" section
documenting a design white-space or an unresolved question.

## Reference
- [[glossary]] — canonical terms (the four affinity aspects, peer sets, …)

## Context & model
- [[v1-limitations]] — why v1 flood gossip caps storage & bandwidth
- [[v2-overview]] — v2 = topic-based DHT routing; what changes vs v1
- [[topic-affinity]] — the two kinds of affinity: DHT vs explicit
- [[node-roles]] — full vs light node; the affinity-aspect matrix

## DHT affinity (#11933)
- [[peers-topology]] — the Peers Topology module; ties the queries together
- [[peer-sets]] — known (discovery) set vs connected (active) set
- [[peer-set-eviction]] — ⚠️ TTL + active-set-cap bounds
- [[peer-connection-model]] — how peers connect: standalone + reserved slots (#12272)
- [[dht-affinity-routing]] — ⚠️ forward to k-closest connected peers
- [[dht-affinity-storing]] — ⚠️ store when among k-closest per routing table
- [[xor-keyspace]] — ⚠️ topic/peer key derivation + XOR distance
- [[routing-table]] — Kademlia k-buckets and their maintenance
- [[discovery-coverage]] — empirical: random-walk discovers >99%; kad table is lossy (#11933)
- [[discovery-piggyback]] — why discovery rides the chain's genesis-scoped DHT (people-chain-bound)

## Explicit affinity (#11934, #11935)
- [[explicit-affinity]] — the explicit affinity set; routing + storing
- [[explicit-affinity-sources]] — RPC subs, CLI/config, extensible
- [[explicit-affinity-discovery]] — local routing-table lookup only, no FIND_NODE
- [[peer-steering]] — ⚠️ reputation-based peerset convergence
- [[premium-prioritisation]] — paid retention via explicit affinity

## Routing & propagation
- [[forwarding-decision]] — per-topic, per-peer forward rule
- [[multi-topic-replication]] — N topics → up to N DHT paths
- [[propagation-termination]] — convergence + duplicate drop
- [[duplicate-tracking]] — per-peer LRU of hashes + reputation penalty
- [[periodic-propagation]] — re-evaluate for late-connecting peers

## Storage (#10910, #11936)
- [[statement-storage]] — affinity check then validation pipeline
- [[store-limits]] — ⚠️ configurable limits per affinity reason
- [[index-locking]] — read/write lock split + disk/LRU staged refactor

## Sync, privacy, client
- [[node-join-sync]] — filtered initial sync respecting peer affinity
- [[bloom-filter-affinity]] — hide explicit affinity via bloom in handshake
- [[privacy-correlation-risk]] — DHT-affine connection correlation + mitigation
- [[client-rpc]] — submission + subscription; subscription auto-adds affinity
- [[light-node-protocol]] — light node send/receive, own routing table, steering
- [[light-node-implementation]] — how the smoldot light node works today
- [[light-node-client-api]] — the light node's JSON-RPC surface
- [[statement-protocol-versioning]] — v1/v2 protocol pair, negotiation, fallback

## Optimisations, orchestration, rollout
- [[duplicate-statement-problem]] — overlapping peers → duplicate traffic
- [[set-reconciliation]] — ⚠️ fewer live connections via reconciliation (deferred)
- [[orchestrator]] — statement-store orchestrator wiring the modules (#11937)
- [[./milestones/DHT_POC]] — PoC implementation tracker: what's built vs. not
- [[rollout-plan]] — ⚠️ migration + coordinated protocol switch (#11938)
- [[simulation]] — storage/replication & traffic/gossip-target findings

## Open questions index (white-spaces)
- [[dht-affinity-routing]] — closest-k **connected** vs **known**; needs research
- [[dht-affinity-storing]] — remote DHT eligibility undecidable from discovery
- [[xor-keyspace]] — peer/topic key hash unspecified (blake2 vs DHT's SHA-256)
- [[peer-set-eviction]] — known-set TTL and active-set cap both unenforced
- [[discovery-coverage]] — staleness eviction unsolved; people-chain-only scope (open participation needs a dedicated DHT)
- [[peer-connection-model]] — reserved/random slot counts & admission policy unspecified
- [[peer-steering]] — scoring algorithm/weights unspecified
- [[store-limits]] — eviction policy when a limit is hit unspecified
- [[set-reconciliation]] — mechanism deferred to future work
- [[rollout-plan]] — migration path & coordination unspecified; v1-fallback removal (no v1 interop)
