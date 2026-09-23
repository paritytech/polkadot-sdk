# Light node client API

The JSON-RPC surface a client uses against a smoldot light node
([[light-node-implementation]]); defined in `lib/src/json_rpc/methods.rs:467`
(smoldot repo). Mirrors the full node's [[client-rpc]] shape, minus anything
requiring a local store.

- `statement_submit(encoded)` — broadcast a SCALE-encoded statement to connected
  v2 peers. Returns `New`, `Invalid` (bad encoding), or an internal error —
  notably `NoConnectedPeers`, since without a store the light node can only
  hand the statement to the network *now*.
- `statement_subscribeStatement(filter)` — subscribe with a `TopicFilter`
  (`"any"`, `matchAll: [...]`, or `matchAny: [...]` — JSON-compatible with the
  full node's); returns a subscription id. Each subscription keeps
  its own seen-LRU, so replacing a subscription re-delivers history the old one
  saw ([[light-node-implementation]]).
- `statement_unsubscribeStatement(id)` — drop the subscription.
- `statement_statement(id, event)` — the notification stream of matching
  statements.

As in the full node's [[client-rpc]], subscribing feeds the node's explicit
affinity: all live subscriptions' filters are unioned into one bloom filter and
advertised to peers ([[bloom-filter-affinity]]). A `TopicFilter::Any`
subscription advertises a match-everything filter (v1-flood equivalent for that
node). The bloom parameters are embedder config: the JS API exposes per-chain
`statementStore: { maxSeenStatements, falsePositiveRate, affinityUpdateIntervalMs }`
(`wasm-node/javascript/src/public-types.ts:472`), while the bloom `seed`
(`bloom_seed: u128`) sits one level down in the Rust `StatementProtocolConfig`
(`light-base/src/network_service.rs:83`). So the privacy/bandwidth trade-off is
the embedder's choice, per §3.7.1.
