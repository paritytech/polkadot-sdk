# Peer connection model

Issue #12272. How a peer is *allowed* to open the statement-store notification
substream — distinct from which peers we choose to gossip to.

## v1: piggybacks on sync
Today a node accepts a statement-store connection only from peers it is already
connected to on the **sync protocol** (the notification config gates on it, and
statement substreams open in response to sync events). There is no way to
connect on "just" the statement protocol.

## v2: standalone, with reserved slots
DHT affinity needs to accept **random** peers connecting on the statement
protocol alone — a peer close to a topic may not be a sync peer. The plan:

- change the notification protocol config to **allow inbound connections** on
  the statement protocol independently of sync, and
- always keep **a few free slots for random connections** so new/unknown peers
  can still get in.

The peers **closest to us in the DHT topology** ([[xor-keyspace]],
[[peers-topology]]) go into the **reserved set**, so they stay always-connected
— this is the mechanism that keeps the [[dht-affinity-routing]] target
neighbourhood reliably connected.

Admission ("which peers we accept") is owned by the [[orchestrator]] (#11937);
the reserved/random slot split lives in the connected (active) set
([[peer-sets]], [[peer-set-eviction]]); non-reserved slots are churned by
[[peer-steering]].

## Open questions (white-space)
The issue states the mechanism but not the parameters:
- how many **reserved** vs **random** slots, and how that scales with the
  connected-set cap,
- the **admission policy** when the random slots are full (reject, evict a
  random peer, or prefer peers closer in [[xor-keyspace]]),
- whether a sync-protocol connection still grants any priority, or the two
  protocols become fully independent.
