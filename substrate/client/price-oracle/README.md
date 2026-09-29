# sc-price-oracle

Node side of the price oracle. Runs inside a block producing node, fetches market data from
exchanges, prices it through the runtime, signs the result, gossips it to the other oracle
nodes, and hands the collected reports to the block author as inherent data.
`pallet-price-oracle` documents the model and the vocabulary; this README covers what the node
does with it.

The crate is chain agnostic. It knows the runtime only through `PriceOracleApi` of
`sp-price-oracle` and the signer key only as a type parameter. A node without a signer key in its
keystore reports nothing, but still relays the reports of others and includes them in the blocks
it authors.

## Position in the stack

```text
exchanges ──HTTPS──▶ fetcher
                        │ bodies
                        ▼
              runtime `parse` ──prices──▶ runtime `aggregate`
                                                  │ quotes
                                                  ▼
                              keystore ──sign──▶ SignedPriceReport
                                                  │
                  ┌──── gossip ◀──────────────────┤
                  ▼                               ▼
            other nodes                      ReportPool
                                                  │ select
                                                  ▼
             block author ──▶ InherentDataProvider ──▶ runtime
```

The host node makes three calls: `peers_set_config` to register the gossip protocol while
building the network, `run` to spawn the service, and `PriceOracleInherentDataProvider::create`
inside its inherent data providers for every block it authors. The service and the provider
share a `ReportPool`.

## One tick

The tick loop runs on its own timer, independent of block import. Runtime API calls read state
at the best block, the freshest state the node knows. During a chain stall the loop keeps
ticking against the last known state, reports accumulate in the pool, and the first block after
the stall carries them.

The tick interval comes from the runtime, re-read every tick so a governance change takes
effect without restart, with a floor of 500 ms.

1. **Read** the signers, the report window, the tick interval and the active markets at the
   best block.
2. **Fetch** all queries of all markets concurrently, under one tick deadline of the tick
   interval minus 200 ms. Each request has its own timeout and response size cap. A market
   whose queries did not all succeed by the deadline is dropped for this tick.
3. **Parse**: as soon as all responses of one market are in, call `parse` for it.
4. **Aggregate**: at the deadline, one call to `aggregate`. An empty result means nothing to
   sign this tick.
5. **Sign** a `PriceReport` anchored to the best block number with the local key.
6. **Publish**: insert into the pool and gossip.

A tick is skipped while the node is major syncing, while the previous tick is still running,
and while the runtime does not implement the API.

## Fetcher

`hyper` with `hyper-rustls` and `hyper-util`'s legacy client. One client for the whole service,
so connections stay pooled across ticks; HTTP/2 where the venue supports it, HTTP/1.1 keep alive
otherwise.

Server certificates are verified against the operating system's root store, which the node
therefore requires. `SSL_CERT_FILE` and `SSL_CERT_DIR` override its location. Without a root
store the service logs an error and does not run; the node is unaffected.

The URL is `https://{host}{path}?{name}={value}&...` with names and values URL encoded; the
scheme is not configurable. Redirects are not followed and a non 2xx status is a failure. A
response larger than the request's `max_response_bytes` is aborted while streaming. Caps that
governance cannot exceed protect the process: host and path 2 KB, response 4 MB, timeout 12 s.

## Gossip

Protocol `/{genesis_hash}[/{fork_id}]/price-oracle/1` on `sc-network-gossip`, 25 in and 25 out
peers, non reserved peers accepted. One topic for all reports, so every node receives every
report. A message is a SCALE encoded `SignedPriceReport`.

The validator runs on every incoming message before it enters the engine. A message is
discarded, and the sending peer's reputation lowered, if it does not decode, is anchored before
the window, comes from a signer outside the accepted set, or carries an invalid signature, in
that order, so the signature is checked last. A report the pool declines as superseded is
discarded without penalty. Anything else is kept, forwarded, and rewarded.

Anchors ahead of the current anchor are accepted: a peer may be ahead of this node, and the
runtime rejects anchors ahead of the block including them. Valid messages are inserted into the
pool by the validator itself. The signer set, the current anchor and the window form a snapshot
the tick loop replaces once per tick, so validation never calls the runtime on the network path.
While the node is major syncing or cannot read the rules from the runtime, the snapshot is
cleared and incoming reports are discarded without judging the sender. A message is expired for
rebroadcast once its anchor falls out of the window.

## Pool

`ReportPool` holds one report per signer, shared by the validator, the tick loop and the
inherent data provider. A report replaces the signer's previous one if its anchor is greater or
equal: at equal anchors nothing tells which is fresher, so the last received wins. The tick loop
prunes reports that fell out of the window. Reading for a block removes nothing, so a report not
included in one block stays available for the next.

## Inherent data

For the block on `parent`, the provider reads the parent's height, `settings` and
`latest_anchors` at the parent, and takes every pooled report anchored within the window at or
below that height, except those anchored before the signer's vote already on chain. No other
filtering: the author includes every fresh report it holds. If the header or the API is
unavailable, the block is authored without oracle data.

## Signing

The local key is the first accepted signer whose private key the keystore holds, looked up every
tick so key rotation and set changes need no restart.

## Open points

- Metrics of the service itself are not registered: tick duration, markets priced and failed,
  reports signed and received, pool size.
- The validator does not bound how far ahead of the current anchor a report may be.
- The service stops if the gossip stream or engine ends, rather than recovering.
