# Price Oracle Pallet

On-chain prices of asset pairs, such as DOT/USD, computed from data the block producers of the
chain fetch from exchanges. The pallet stores what to fetch, prices the fetched data through
runtime APIs, and aggregates the prices the nodes sign and report. Each accepted signer holds one
vote per pair, and the price of a pair is the median of its votes once enough signers have
reported. Governance registers exchanges, pairs and markets, and can pause a pair without a
runtime upgrade. Consumers read prices through `frame_support::traits::PriceProvider`.

How a market is priced is up to the runtime. The pallet ships one method, the execution band.

## Data flow

```text
governance ──venues, pairs, markets──▶ storage
                                           │ markets, settings
                                           ▼
                                      oracle nodes
                                           │ responses
                                           ▼
                         runtime APIs `parse`, `aggregate`
                                           │ quotes
                                           ▼
                                     signed report
                                           │ delivery, inherent
                                           ▼
block author ──▶ process_reports ──filter, vote, median──▶ Prices
                                                             │
                                PriceProvider, OnPriceUpdate │
                                                             ▼
                                                         consumers
```

## Vocabulary

- **Pair**: two assets whose exchange rate is priced, such as DOT/USD. The price is in quote
  asset units per one base asset unit. Pairs are registered by governance and identified by a
  `PairId`.
- **Venue**: an exchange the oracle fetches from, such as Binance.
- **Market**: one instrument of one pair on one venue, such as DOT/USDT spot on Binance. A market
  defines what to fetch and how to price it.
- **Query**: one HTTPS request of a market, identified by a `QueryTag`.
- **Pricing method**: how a market is priced from the responses to its queries. The runtime
  chooses it through `Config::MarketPricing`.
- **Cross rate**: a way to price a pair from two other pairs, such as DOT/USD as
  DOT/USDT * USDT/USD.
- **Signer**: a key whose reports the pallet accepts. The set of signers comes from the runtime,
  typically the block producers.
- **Report**: the pair prices one signer computed in one pass over the markets, signed and
  anchored.
- **Anchor**: when a report was made: the height of the signer's best block and the signer's
  clock. Of two reports of a signer, the one with the higher block is newer, and at the same block
  the one with the later time. Anchors also expire old reports.
- **Vote**: the latest price one signer reported for one pair. A pair holds at most one vote per
  signer.
- **Quorum**: what a pair needs to have a price. There are two. The market quorum of a pair is the
  fewest markets a node needs before reporting the pair, and the signer quorum of `Params` is the
  fewest votes the pallet needs before publishing it.

## How it works

1. An oracle node reads the accepted signers, the report window, the tick interval and the active
   markets from the runtime, and fetches the queries of every market.
2. The node prices each market and aggregates the market prices into pair prices through the
   runtime APIs, so the rules are the runtime's and governance changes them without a node
   release.
3. The node signs the pair prices as a report anchored to its best block and its clock, and sends it
   to the nodes authoring next.
4. A block author includes the reports it collected as an inherent. The inherent never rejects a
   block: if it fails, the block keeps no oracle update.
5. The pallet drops reports that are stale, from an unknown signer or wrongly signed, replaces
   each signer's vote with its newer one, and publishes the median of every pair with enough
   votes.

The runtime APIs are `PriceOracleApi` of `sp-price-oracle`. A runtime implements them with the
pallet's `settings`, `latest_anchors`, `active_markets`, `parse_market` and `aggregate_markets`.
The node side is implemented in `sc-price-oracle`.

## Setting up

Every call is made by `Config::AdminOrigin`. To price DOT/USD from Binance DOT/USDT spot and the
USDT/USD rate:

```text
set_venue(BINANCE, Venue { name: "Binance" })
set_pair(DOT_USDT, PairConfig { market_quorum: 3 }) // needs 3 markets
set_pair(USDT_USD, PairConfig { market_quorum: 2 })
set_pair(DOT_USD, PairConfig { market_quorum: 3 })
set_cross_rates(DOT_USD, [(DOT_USDT, USDT_USD)])    // DOT/USD = DOT/USDT * USDT/USD
set_market(0, StoredMarket { venue: BINANCE, pair: DOT_USDT, pricing, active: true })
set_parameters(Parameters { report_window, report_window_ms, quorum, tick_interval_ms })
```

Nothing is reported or published until the parameters are set.

## Pricing a market

The pricing method is a type implementing `MarketPricing`. From the parameters stored with a
market, it defines the queries to fetch, and prices the market from their responses. It also
aggregates the market outputs into pair prices, and may keep a state from one tick to the next,
such as the last prices. The node holds that state between ticks without reading it. Before
pricing, the pallet only checks that each response belongs to one of the market's queries and is
not larger than the query allows.

```ignore
impl pallet_price_oracle::Config for Runtime {
	type MarketPricing = pallet_price_oracle::order_book::OrderBookPricing;
	// ...
}
```

A runtime can bring its own method, for example one that reads the last price from a ticker
response.

## Execution band pricing

The pallet ships one pricing method, the execution band, as `order_book::OrderBookPricing`. The
algorithm is described step by step in the docs of the `order_book` module. In short:

- Each market gives an interval: the average prices at which `impact_size` quote units are sold
  into the bids, the lower edge, and bought from the asks, the upper edge. A side that cannot fill
  has no edge. A market is left out if its book is empty, crossed or its trades are stale; a wide
  spread does not leave it out.
- A pair's band is the median of its markets' lower edges, `L`, and the median of their upper
  edges, `U`, if it has at least its market quorum of markets and more than half of them fill each
  side. The markets of a cross rate count with both edges multiplied by this tick's price of the
  rate.
- The price is the node's previous price if that is inside the band, otherwise the nearest edge of
  the band. Without a previous price, it is the middle of the band. A pair without a band is left
  out of the report and keeps its previous price. The previous prices are the pricing state,
  which the node passes from each tick to the next.

### Reading responses

Each response is read with the `ResponseSchema` of its query, a path into the JSON document plus
the layout of an order book level or the timestamp field of a trade. Any malformed element
rejects the whole response. For example, this response and schema:

```text
{"bids": [["4.00", "1000"], ["3.99", "5000"]], "asks": [["4.02", "1000"], ["4.03", "5000"]]}

ResponseSchema::OrderBook { bids: ["bids"], asks: ["asks"], layout: Array { price: 0, amount: 1 } }
```

The `pallet-price-oracle-venues` crate has tested market definitions for well-known exchanges.

## Processing reports

The block author includes the inherent `process_reports` with the reports it collected in every
block it builds, possibly with none. It is an operational inherent, not a mandatory one: a block
without it is valid, and processing happens at most once per block.
Nothing is processed while `Params` is unset or processing is paused. A report is ignored if it
is not live, if its signer is not in `Signers::signers()`, or if its signature does not verify.
Of several reports of one signer in a block, the one with the highest anchor is kept. At equal
anchors the later one in the block wins.

A report or vote is live while:

- its block is not ahead of the current block of `AnchorProvider`, and at most `report_window`
  blocks behind it;
- its timestamp is at most `report_window_ms` behind the current time of `AnchorProvider`.

A timestamp ahead of the chain's time is accepted: the chain's time may lag behind the signer's
clock, and the block window bounds how long such a vote counts. The chain's time only ages votes: running
ahead of real time expires them early, lagging behind lets them count longer. It keeps advancing
while this chain stalls, so votes made before a stall expire after it.

Each quote of an accepted report becomes the signer's vote for that pair, replacing an older or
equally anchored vote. Votes that are no longer live are dropped. A pair with at least
the signer `quorum` of `Params` gets the median as its price, stamped with the current block
number of `BlockNumberProvider` and the number of votes. When the price differs from the stored
one, `PriceUpdated` is emitted and `OnPriceUpdate` is called, unless publishing is paused. The
inherent fails only if it carries more reports than `MaxSigners` or comes twice in a block; the
block stays valid either way. Rejected reports are counted in `ReportsProcessed`.
