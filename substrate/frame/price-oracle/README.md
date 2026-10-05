# Price Oracle Pallet

On-chain prices of asset pairs, such as DOT/USD, computed from data the block producers of the
chain fetch from exchanges. The pallet stores what to fetch, prices the fetched data through
runtime APIs, and aggregates the prices the nodes sign and report. Each accepted signer holds one
vote per pair, and the price of a pair is the median of its votes once enough signers have
reported. Governance registers exchanges, pairs and markets, and can pause the pallet without a
runtime upgrade. Consumers read prices through `frame_support::traits::PriceProvider`.

How a market is priced is up to the runtime. The pallet ships one method, order book pricing.

## Position in the stack

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
                                           │ gossip, inherent
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
  anchored to a block height.
- **Anchor**: the block height a report is stamped with. It orders the reports of a signer and
  expires old ones.
- **Vote**: the latest price one signer reported for one pair. A pair holds at most one vote per
  signer.
- **Quorum**: the number of votes a pair needs to have a price. There are two. The market quorum
  of a pair is applied by a node before reporting the pair, and the signer quorum of `Params` is
  applied by the pallet before publishing it.

## How it works

1. An oracle node reads the accepted signers, the report window, the tick interval and the active
   markets from the runtime, and fetches the queries of every market.
2. The node prices each market and aggregates the market prices into pair prices through the
   runtime APIs, so the rules are the runtime's and governance changes them without a node
   release.
3. The node signs the pair prices as a report anchored to the current block height and gossips it
   to the other oracle nodes.
4. A block author includes the reports it collected as an inherent. The inherent never rejects a
   block.
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
set_pair(DOT_USDT, 3)                               // needs 3 market votes
set_pair(USDT_USD, 2)
set_pair(DOT_USD, 3)
set_cross_rates(DOT_USD, [(DOT_USDT, USDT_USD)])    // DOT/USD = DOT/USDT * USDT/USD
set_market(0, StoredMarket { venue: BINANCE, pair: DOT_USDT, queries, pricing, active: true })
set_parameters(Parameters { report_window, quorum, tick_interval_ms })
```

Nothing is reported or published until the parameters are set. A pair can only be removed once
no market quotes it and no cross rate uses it.

## Pricing a market

The pricing method is a type implementing `MarketPricing`. It gets the responses to a market's
queries and the parameters stored with the market, and returns a price or an error. Before
calling it, the pallet only checks that each response belongs to one of the market's queries and
is not larger than the query allows.

```ignore
impl pallet_price_oracle::Config for Runtime {
	type MarketPricing = pallet_price_oracle::order_book::OrderBookPricing;
	// ...
}
```

A runtime can bring its own method, for example one that reads the last price from a ticker
response, with the path to the price field as its parameters.

### Order book pricing

`order_book::OrderBookPricing` prices a market at the impact mid of its order book. That is the
average of the prices at which `impact_size` quote units are bought from the asks and sold into
the bids. Amounts quoted in contracts are first converted to base asset units with the market's
`contract_size`.

A market is not priced if it fails one of its `HealthLimits`:

- its best bid is not below its best ask,
- its spread relative to the mid exceeds `max_spread`,
- its latest trade is older than `max_trade_age_ms`,
- or a side of the book cannot fill `impact_size`.

Each response is read with the `ResponseSchema` of its query, a path into the JSON document plus
the layout of an order book level or the timestamp field of a trade. Any malformed element
rejects the whole response. For example, this response and schema:

```text
{"bids": [["4.00", "1000"], ["3.99", "5000"]], "asks": [["4.02", "1000"], ["4.03", "5000"]]}

ResponseSchema::OrderBook { bids: ["bids"], asks: ["asks"], layout: Array { price: 0, amount: 1 } }
```

The `pallet-price-oracle-venues` crate has tested market definitions for well-known exchanges.

## Aggregating markets into pairs

Every priced market is one vote for its pair. A pair with cross rates also gets one vote per
market of each `source` pair, multiplied by the median price of the `rate` pair. A rate is taken
from its own markets only. A pair is reported with the median of its votes if it has at least
its market quorum, and is left out of the report otherwise.

For example, with two DOT/USDT markets at 4.00 and 4.02, a USDT/USD price of 0.999 and one DOT/USD
market at 4.01, DOT/USD has the votes 3.996, 4.01598 and 4.01, and is reported at 4.01.

## Processing reports

The block author's reports arrive in the mandatory inherent `process_reports`, once per block.
Nothing is processed while `Params` is unset or processing is paused. A report is ignored if its
anchor is ahead of the current one or more than `report_window` blocks behind it, if its signer
is not in `Signers::signers()`, or if its signature does not verify. Of several reports of one
signer in a block, the one with the highest anchor is kept. At equal anchors the later one in the
block wins.

Each quote of an accepted report becomes the signer's vote for that pair, replacing an older or
equally anchored vote. Votes anchored outside the report window are dropped. A pair with at least
the signer `quorum` of `Params` gets the median as its price, stamped with the current block
number of `BlockNumberProvider` and the number of votes. When the price differs from the stored
one, `PriceUpdated` is emitted and `OnPriceUpdate` is called, unless publishing is paused. The
inherent never fails, and rejected reports are counted in `ReportsProcessed`.

## Open points

- Anchors are block numbers of this chain, which stop advancing while the chain stalls, so
  reports made before a stall look fresh after it. A two-dimensional anchor, adding the relay
  chain slot, is a candidate replacement.
