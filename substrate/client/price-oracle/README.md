# sc-price-oracle

Node side of the price oracle. Runs inside a block producing node, fetches market data from
exchanges, prices it through the runtime, signs the result, sends it to the nodes authoring
next, and hands the collected reports to the block author as inherent data.
`pallet-price-oracle` documents the model and the vocabulary; this README covers what the node
does with it.

The crate is chain agnostic. It knows the runtime only through `PriceOracleApi` of
`sp-price-oracle` and the signer key only as a type parameter. A node without a signer key in its
keystore reports nothing, but still receives the reports of others and includes them in the
blocks it authors.

## Position in the stack

```text
exchanges ──HTTPS──▶ fetcher
                        │ bodies
                        ▼
              runtime `parse` ──outputs──▶ runtime `aggregate`
                                                  │ quotes
                                                  ▼
                              keystore ──sign──▶ SignedPriceReport
                                                  │
                  ┌──── send ◀────────────────────┤
                  ▼                               ▼
           next authors                      ReportPool
                                                  │ select
                                                  ▼
             block author ──▶ InherentDataProvider ──▶ runtime
```

The host node makes three calls: `peers_set_config` to register the protocol while building
the network, `run` to spawn the service, and `PriceOracleInherentDataProvider::create` inside
its inherent data providers for every block it authors. The service and the provider share a
`ReportPool`. The host also tells the service where to send reports, through an implementation
of `ReportRecipients`: the peer addresses of the nodes expected to author soon.

## One tick

The tick loop runs on its own timer, independent of block import. Runtime API calls read state
at the best block, the freshest state the node knows. During a chain stall the loop keeps
ticking against the last known state, reports accumulate in the pool, and the first block after
the stall carries them.

The tick interval comes from the runtime, re-read every tick so a governance change takes
effect without restart, with a floor of 500 ms.

1. **Read** the signers, the report windows, the tick interval and the active markets at the
   best block.
2. **Fetch** all queries of all markets concurrently, under one tick deadline of the tick
   interval minus 200 ms. Each request has its own timeout and response size cap. A market
   whose queries did not all succeed by the deadline is dropped for this tick.
3. **Parse**: as soon as all responses of one market are in, call `parse` for it.
4. **Aggregate**: at the deadline, one call to `aggregate` with the market outputs and the state
   the runtime returned last tick. It returns every registered pair with its inputs, quorum, price
   if it has one, a note, and the state for the next tick. The node keeps the state in memory
   for the next tick only, without reading it.
5. **Sign** a `PriceReport` with the local key, anchored to the best block number and the
   node's clock at signing.
6. **Publish**: insert into the pool and send to the recipients.

A tick is skipped while the node is major syncing, while the previous tick is still running,
and while the runtime does not implement the API. A skipped tick stops only the node's own
report. Reports from peers are still received under the rules of the last tick, except while
the node is syncing.

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

## Delivery

Protocol `/{genesis_hash}[/{fork_id}]/price-oracle/1`. A message is a SCALE encoded
`SignedPriceReport`. There is no gossip: a report goes straight from its signer to the nodes
that author next, and nothing is forwarded.

Every tick, the service asks its `ReportRecipients` for the addresses of the coming authors and
reserves them on the protocol, so the network keeps a substream open to each. The signed report
is sent to every recipient whose substream is open at that moment. While the node is itself a
recipient, the last few peers that stopped being recipients stay reserved: a substream is shared
by both sides, and a node that drops a peer from its reservation closes the substream it opened,
while the peer is still sending its reports over it. An author therefore receives the reports of
all signers directly, and a signer talks to a handful of peers. The protocol dials only the
reserved peers, and accepts an inbound substream only from a peer the `ReportRecipients` knows
as an authority, so that other peers cannot take the inbound slots.

`polkadot-omni-node` sends to the authors of the Aura slot of the best block and the next
three, with the authorities from the best block, and keeps the last four past ones reserved
while it is one of the four itself. The authors are found through the parachain's authority
discovery, so the runtime must return its authority discovery keys in authoring order, as
`cumulus-client-collator-discovery` requires.

Every incoming message is validated. A message is rejected, and the sending peer's reputation
lowered, if it does not decode, is anchored before a window, comes from a signer outside the
accepted set, is not newer than the report the pool holds of that signer, or carries an invalid
signature, in that order, so the signature is checked last and a replayed report never reaches
it. Anything else is pooled and rewarded.

A report is within the windows if its block is at most `report_window` behind the tick's block
and its timestamp at most `report_window_ms` behind the node's clock. Blocks and timestamps ahead
are accepted: a peer may be ahead of this node, and the runtime rejects blocks ahead of the block
including them. The node's clock is used rather than the chain's time, which lags behind. The signer
set, the tick's block and the windows form a snapshot the tick loop replaces once per tick, so
validation never calls the runtime on the network path.

## Pool

`ReportPool` holds one report per signer, shared by the tick loop and the inherent data provider.
A report replaces the signer's previous one if its anchor is greater or equal, comparing blocks
first, then timestamps: at equal anchors the last received wins. The tick loop prunes reports that
fell out of either window. Reading for a block removes nothing, so a report not included in one
block stays available for the next.

## Inherent data

For the block on `parent`, the provider reads the parent's height, `settings` and
`latest_anchors` at the parent, and takes every pooled report anchored within the block window
of the block being built and within the time window of the node's clock, except those anchored
before the signer's vote already on chain. The clock filter is stricter than the runtime after a
relay chain stall, when the chain's time still shows the time before it. If the header or the API
is unavailable, the block is authored without oracle data.

## Signing

The local key is the first accepted signer whose private key the keystore holds, looked up every
tick so key rotation and set changes need no restart.
