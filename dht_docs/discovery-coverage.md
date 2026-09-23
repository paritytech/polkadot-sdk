# Discovery coverage (empirical)

Findings from a 1000-node experiment (substrate discovery + litep2p) by the
design author, recorded on #11933 (2026-06-04); each node settled to ~50
simultaneous connections.

## A single node discovers almost the whole network
Periodic random-walk FIND_NODE lookups ([[routing-table]]) reach **>99%** of a
1000-node network — the key is random and the query stops when connected peers
return nothing closer. So `RoutingTableUpdate` fires for most existing nodes.
Time to 99%: **~86 queries**, one every 5s → **~7 minutes**.

## The Kademlia routing table does NOT hold every node
Each k-bucket caps at 20, so most discovered nodes are discarded: for 1000 nodes
the litep2p kad table settles at only **~130** entries. The internal routing
table is therefore **not** a complete membership source — the topology must keep
its own record of every peer it saw a `RoutingTableUpdate` for. This is why the
known set ([[peer-sets]]) is a deliberate **superset** of the kademlia routing
table, tracked separately rather than mirrored from it.

## Protocol support is learned for most nodes anyway
Discovering ~99% of the network also means having **connected to ~88%** of it
along the way (FIND_NODE dials peers others return). So handshake/identify data —
and thus DHT eligibility ([[dht-affinity-storing]]) — is obtained for most nodes
over time, without targeted lookups.

## Scope: good for a closed set, not open participation
For the PoC and **as long as only people-chain nodes** run the statement store,
this is good enough to build a whole-network view. It will **not** scale to
arbitrary nodes participating — that needs a dedicated statement-store
Kademlia/DHT network. The mechanism and the genesis-hash boundary behind this
are in [[discovery-piggyback]].

## Open questions
- Coverage ≠ freshness: the experiment shows near-complete discovery but not how
  dead nodes leave the known set ([[peer-set-eviction]]).
- The people-chain-only assumption is a hard boundary; open participation is
  unsolved and deferred to a dedicated DHT.
