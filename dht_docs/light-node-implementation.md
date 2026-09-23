# Light node implementation (smoldot)

How the [[light-node-protocol]] design is implemented today. Code lives in the
smoldot fork at `smoldot/` (paths below are from that repo's root); the
statement work landed via #3127 (basic support), #3151 (bloom affinity), #3163
(per-subscription dedup).

Pipeline, bottom-up:

- **Codec** — `lib/src/network/codec/statement.rs:51` defines the same
  `StatementMessage` as the full node: `Statements(...)` and
  `ExplicitTopicAffinity(AffinityFilter)`. A light node *sends* filters but never
  expects to receive them (it doesn't route). The bloom filter
  (`lib/src/network/codec/affinity.rs`) is wire-identical to the full node's
  ([[bloom-filter-affinity]]): `seed: u128` + `num_hashes: u32` (LE) + compact
  length + `u64` LE words, same 1 MiB bit cap and 64-hash max, hashing pinned so
  bits match between wasm32 and 64-bit targets.
- **Network service** — `light-base/src/network_service.rs` tracks which
  connected peers negotiated v2 (`v2_statement_peers`, line 1070) and the current
  local filter (`current_affinity_filter`, line 1073). On a v2 substream opening
  it immediately sends the filter (line 3281); on a filter change it stores the
  new filter and re-sends it to every v2 peer (lines 2157–2159); recomputes are
  debounced by a configurable `affinity_update_interval` so subscription churn
  doesn't spam peers. Incoming
  `Statements` notifications are surfaced as events; incoming peer filters are
  ignored (line 3296) since a light node has nothing to forward.
- **JSON-RPC service** — `light-base/src/json_rpc_service/statement.rs` owns the
  client-facing logic ([[light-node-client-api]]): per-subscription topic
  filters, the union bloom filter (`build_combined_affinity_filter`, line 81),
  and per-subscription LRU dedup (`accept`, line 68). Dedup lives here, not in
  the network layer, so a *new* subscription still receives statements an older
  one already consumed (#3163).

Receive flow: full node matches our advertised filter → pushes statements →
network service surfaces them → each subscription's `accept` checks its exact
`TopicFilter` (bloom false positives are discarded here) and its LRU →
matching statements stream to the client.

Submit flow: `validate_and_broadcast_statement`
(`light-base/src/json_rpc_service/statement.rs:30`) decodes the statement
(encoding check only — a light node runs no store validation) and broadcasts it
to **all gossip-connected peers** (`network_service.rs:2123`), not just the ones
that negotiated the v2 statement protocol; zero connected peers is an error. There is no
local statement store: submitting is fire-and-forget toward full nodes, per the
design's trust model. Submissions are not re-tried after a peer rejects (#3230).

e2e-verified behaviours (`e2e-tests/tests/`): a submitted statement reaches a
full node; only subscribed statements are received; delivery recovers after the
peer restarts; browser ping-pong.

What the design describes but the implementation does not yet do — the Stage 4
gap ([[light-node-protocol]], [[peer-steering]]):

- no routing-table-based peer selection: statement peers come from smoldot's
  generic slot assignment, not topic-aware DHT lookups;
- no peer steering (no scoring, no churn toward better providers);
- submission broadcasts to all v2 peers instead of targeting topic-affine ones;
- protocol errors are logged but the peer is not disconnected
  (`light-base/src/network_service.rs:3299`).

Protocol version negotiation and the v1 fallback are covered in
[[statement-protocol-versioning]].
