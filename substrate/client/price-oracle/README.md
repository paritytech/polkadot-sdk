# sc-price-oracle

Node side of the price oracle. Runs inside a block producing node (a parachain collator or a
relay chain validator), fetches market data from exchanges, prices it through the runtime,
signs the result, gossips it to the other oracle nodes, and hands the collected reports to the
block author as inherent data.

The crate is chain agnostic. It knows the runtime only through the two runtime APIs of
`sp-price-oracle`, the signer key only as a type parameter, and never depends on the pallet.

> Status: implemented, not yet wired into a node.

## Position in the stack

```
exchanges ──HTTPS──▶ fetcher ──bodies──▶ runtime `parse` ──prices──▶ runtime `aggregate`
                                                                          │ quotes
                                                                          ▼
                                                    keystore ──sign──▶ SignedPriceReport
                                                                          │
                                    ┌──── gossip (sc-network-gossip) ◀────┤
                                    ▼                                     ▼
                              other nodes                            ReportPool
                                                                          │ drain
                                                                          ▼
                                            block author ──▶ InherentDataProvider ──▶ runtime
```

## Modules

| Module | Responsibility |
|---|---|
| `lib.rs` | `Config`, `run()` entry point, the tick loop, wiring of the modules |
| `fetcher.rs` | HTTPS client, URL assembly, concurrent requests with per request timeout and size cap |
| `gossip.rs` | Protocol name and config, message topic, the `Validator`, reputation costs |
| `pool.rs` | `ReportPool`: latest report per signer, pruning, draining for the author |
| `inherent.rs` | Builds the inherent data from the pool for one block, with the author filter |
| `signer.rs` | Finds the local oracle key in the keystore, signs a report |

## Public API

```rust
/// Parameters of `run`.
pub struct Params<Client, Net, SyncService, Id, Signature> {
	pub client: Arc<Client>,          // ProvideRuntimeApi + HeaderBackend
	pub network: Net,                 // sc_network_gossip::Network, Clone
	pub sync: SyncService,            // sc_network_gossip::Syncing + SyncOracle, Clone
	pub notification_service: Box<dyn NotificationService>,
	pub protocol_name: ProtocolName,
	pub keystore: KeystorePtr,
	pub pool: ReportPool<Id, Signature>,   // shared with the inherent data provider
	pub prometheus_registry: Option<Registry>,
}

/// The service. Spawned once by the host node, runs until the network shuts down.
pub async fn run<Block, Client, Net, SyncService, Id, Signature>(params: Params<...>);

/// Registers the notification protocol. Called by the host node while building the network.
pub fn peers_set_config<Block, Net>(genesis_hash, fork_id, metrics, peer_store)
	-> (Net::NotificationProtocolConfig, Box<dyn NotificationService>, ProtocolName);

/// Called by the host node inside `create_inherent_data_providers` for every block it authors.
impl PriceOracleInherentDataProvider {
	pub fn create<Block, Client, Id, Signature>(client, pool, parent_hash)
		-> sp_price_oracle::inherents::InherentDataProvider<Id, Signature>;
}
```

Three entry points: one to register the protocol, one to spawn the service, one to fill
inherent data.

## One tick

The tick loop runs on its own timer, independent of block import. Runtime API calls need a
block to read state from; "at the best block" below means the latest block this node knows,
which is the freshest state available, not a trigger. During a chain stall the loop keeps
ticking against the last known state, reports accumulate in the pool, and the first block
after the stall carries them. (With block number anchors, all reports a signer produces
during a stall share one anchor; the last received wins, see Pool.)

The tick interval comes from the runtime (`tick_interval_ms` at the best block), re-read every
tick so a governance change takes effect without restart. The node clamps it to a floor of
500 ms as a safety cap.

1. **Read the market list**: `markets()` at the best block. Re-read every tick; the list is
   small and this keeps the node in sync with governance edits without a restart.
2. **Fetch**: all queries of all markets are started concurrently under one tick deadline of
   `tick_interval_ms − 200 ms`. Each request has its own timeout and response size cap from
   the `Request`. A market whose queries did not all succeed by the deadline is dropped for
   this tick.
3. **Parse**: as soon as all responses of one market are in, call `parse(market, responses,
   now_ms)` at the best block. Failures are logged at debug level with the runtime's reason. Calls run
   concurrently on the runtime API, one per market.
4. **Aggregate**: at the deadline, `aggregate(prices)` once, at the best block. Empty result
   means nothing to sign this tick.
5. **Sign**: build `PriceReport { anchor, quotes }` with `anchor` = best block number, sign
   with the local key (see Signing), producing a `SignedPriceReport`.
6. **Publish**: insert into the local pool and gossip.

If the runtime at the best block does not implement the two APIs, the service logs an error
once and idles until it does. If the node is major syncing, ticks are skipped.

## Fetcher

- `hyper` with `hyper-rustls` (HTTPS only, webpki roots) and `hyper-util`'s legacy client, the
  same stack `sc-offchain` uses. One client for the whole service so connections stay in the
  pool and are reused across ticks; HTTP/2 where the venue supports it, HTTP/1.1 keep alive
  otherwise. Pool idle timeout well above the tick interval.
- URL assembly: `https://{host}{path}?{name}={value}&...`, values percent encoded by the node.
  The scheme is not configurable.
- Hard safety caps that governance cannot exceed: host and path 2 KB, response 4 MB, timeout
  12 s. These protect the process; the per request values in the `Request` are the operating
  limits.
- Redirects are not followed. Non 2xx status is a failure.
- A response larger than the market's `max_response_bytes` is aborted while streaming, not
  after download.

## Gossip

- Protocol name `/{genesis_hash}[/{fork_id}]/price-oracle/1`, via `sc-network-gossip`'s
  `GossipEngine`, 25 in / 25 out peers, non reserved peers accepted.
- One topic for all reports (a fixed hash), so every node receives every report.
- Message = SCALE encoded `SignedPriceReport`.
- **Validator**, run on every incoming message before it enters the engine:

  | Check | On failure |
  |---|---|
  | decodes as `SignedPriceReport` | discard, cost `MALFORMED` (-500) |
  | anchor ≥ current anchor − window | discard, cost `STALE_REPORT` (-50) |
  | signer ∈ `signers()` at the best block | discard, cost `UNKNOWN_SIGNER` (-150) |
  | signature verifies | discard, cost `BAD_SIGNATURE` (-100) |
  | anchor ≥ the pool's anchor for this signer | discard, no cost (superseded) |
  | otherwise | keep and forward, benefit `GOOD_REPORT` (+100) |

  Cheap checks run before the signature verification. Anchors ahead of the current anchor are
  accepted: a peer may be ahead of this node, and the runtime rejects anchors ahead of the
  block including them. A TODO in the code notes the option of bounding the lead.

  Valid messages are inserted into the pool by the validator itself, so the pool is up to date
  the moment a message is accepted, and the engine forwards them to peers.
- `message_expired`: a report anchored before the current anchor minus the window, so the
  engine drops it from its rebroadcast set.
- The signer set, the current anchor and the window form an `Acceptance` snapshot the tick loop
  replaces once per tick, so validation never calls the runtime on the network path.

## Pool

`ReportPool<Id, Signature>`: `Arc<RwLock<BTreeMap<Id, SignedPriceReport>>>`, one report per
signer. Cheap to clone, shared between the validator, the tick loop, and the inherent data
provider.

- `insert(report) -> bool`: replaces the signer's report if the anchor is greater **or equal**,
  true if it replaced or added. At equal anchors nothing tells which report is fresher, so the
  one received last wins. The same rule applies in the runtime. Byte identical replays never
  reach the pool, the gossip engine drops known messages first.
- `prune(oldest)`: drops reports anchored before `oldest`. The tick loop passes the current
  anchor minus the window.
- `select(on_chain, oldest, newest) -> Vec<SignedPriceReport>`: all reports anchored within
  `oldest..=newest`, except those anchored before the signer's vote already on chain per
  `on_chain`. Does not remove anything from the pool.

## Inherent data

`PriceOracleInherentDataProvider::create(client, pool, parent_hash)` reads the parent's height,
calls `report_window()` and `latest_anchors()` at the parent, and takes
`pool.select(latest_anchors, height − window, height)`. The upper bound keeps reports anchored
ahead of the parent out of the block. The result is wrapped in the provider type from
`sp-price-oracle`. No other filtering: the author includes every fresh report it holds. If the
header or the runtime API is unavailable, an empty provider is returned and the block is
authored without oracle data.

## Signing

The local oracle key is the intersection of `signers()` and the keystore's public keys of type
`Id::ID`, first match. Checked every tick, so key rotation and set changes need no restart. If
there is no local key, the node fetches nothing, but still runs gossip and the pool, so it can
serve as an author for others' reports. Signing goes through `Keystore::sign_with`, the
payload is `PriceReport::signing_payload()`.

## Anchor

Anchor = best block number of the local client at signing time. This matches the pallet's
`AnchorProvider = System` wiring. If the anchor later becomes the relay slot, only step 5 of
the tick and the validator's window check change; the node computes a relay slot from its
clock and the slot duration exposed by the runtime.

## Metrics

Not implemented yet. Only the gossip engine's own metrics are registered. Planned:
tick duration, markets priced and failed by reason, reports signed, reports received by
validation outcome, pool size.

## Host node integration (for later, out of scope of this crate)

- Omni node: cargo feature `price-oracle`, CLI `--enable-price-oracle`, following the statement
  store pattern in `NodeExtraArgs`. Registers the protocol in `build_network`, spawns `run`,
  adds `PriceOracleInherentDataProvider::create` to the collator's inherent data providers.
- Polkadot service: the same three calls at the corresponding places.

## Decisions taken during implementation

1. A node without a local oracle key runs the service, relaying and authoring reports of others.
2. The tick margin (200 ms) and the tick interval floor (500 ms) are constants of the crate.
3. The inherent carries every selected report; the pool holds at most one per signer and the
   runtime bounds votes by `MaxSigners`, so no further cap is applied.
4. Reputation costs and benefits use BEEFY's values.
