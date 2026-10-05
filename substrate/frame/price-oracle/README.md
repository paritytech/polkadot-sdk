# Price Oracle Pallet

On-chain prices of asset pairs, such as DOT/USD, computed from data the block producers of the
chain fetch from exchanges. The pallet stores what to fetch and how to read it, prices the fetched
data through runtime APIs, and aggregates the prices the nodes sign and report. Each accepted
signer holds one vote per pair; the price of a pair is the median of its votes once enough signers
have reported. Governance registers exchanges and markets, sets the health limits of each pair,
and can pause the pallet without a runtime upgrade. Consumers read prices through
`frame_support::traits::PriceProvider`.

## Position in the stack

```text
governance ──venues, markets, limits──▶ storage
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
- **Market**: one pair traded on one venue, such as DOT/USDT on Binance spot. A market carries
  the queries needed to price it and the contract size of the instrument.
- **Query**: one HTTPS request of a market together with the schema that reads its response.
  Which queries a market needs depends on how markets are priced; currently an order book and
  recent trades, see [Pricing a market](#pricing-a-market).
- **Health limits**: the settings of a pair that a market must pass to be priced. The set of
  limits depends on how markets are priced, see [Pricing a market](#pricing-a-market).
- **Signer**: a key whose reports the pallet accepts. The set of signers comes from the runtime,
  typically the block producers.
- **Report**: the pair prices one signer computed in one pass over the markets, signed and
  anchored to a block height.
- **Anchor**: the block height a report is stamped with. It orders the reports of a signer and
  expires old ones.
- **Vote**: the latest price one signer reported for one pair. A pair holds at most one vote per
  signer.
- **Quorum**: the number of votes a pair needs to have a price. There are two: the market quorum
  a node applies before reporting a pair, and the signer quorum the pallet applies before
  publishing it.

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

## Pricing a market

A market is priced from its order book and the time of its latest trade. The book is checked
against the health limits of the pair (`PairSettings`) and rejected if the best bid is not below
the best ask, the spread relative to the mid exceeds `max_spread`, the latest trade is older than
`max_trade_age_ms`, or a side cannot fill `impact_size`. A market that passes is priced at its
impact mid: the mean of the average price at which `impact_size` quote units are bought from the
asks and sold into the bids. Amounts quoted in contracts are scaled to base asset units by the
market's `contract_size` before pricing.

Responses are read with the `ResponseSchema` stored in each query: a path into the JSON document
and, for an order book, the layout of one level, or, for trades, the timestamp field and its
format. Any malformed element rejects the whole response.

This is the method the runtime APIs implement today. It is runtime code, so a different method,
or different queries and limits, is a runtime upgrade.

## Aggregating markets into pairs

Every priced market is one vote for its pair. A pair can also be priced from two other pairs.
For example, DOT/USD can be priced as DOT/USDT * USDT/USD, so each DOT/USDT market also counts
as a DOT/USD vote, multiplied by the USDT/USD price. That USDT/USD price comes from USDT/USD
markets only. A pair is reported with the median of its votes if it has at least the market
`quorum` of its `PairSettings`, and is left out of the report otherwise.

## Processing reports

The block author's reports arrive in the mandatory inherent `process_reports`, once per block.
Nothing is processed while `Params` is unset or processing is paused. A report is ignored if its
anchor is ahead of the current one or more than `report_window` blocks behind it, if its signer
is not in `Signers::signers()`, or if its signature does not verify. Of several reports of one
signer in a block, the one with the highest anchor is kept; at equal anchors the later one in the
block wins.

Each quote of an accepted report becomes the signer's vote for that pair, replacing an older or
equally anchored vote. Votes anchored outside the report window are dropped. A pair with at least
`quorum` votes gets the median as its price, stamped with the current block number of
`BlockNumberProvider` and the number of votes. When the price differs from the stored one,
`PriceUpdated` is emitted and `OnPriceUpdate` is called, unless publishing is paused. The
inherent never fails: rejected reports are counted in `ReportsProcessed`.

## Open points

- Anchors are block numbers of this chain, which stop advancing while the chain stalls, so
  reports made before a stall look fresh after it. A two-dimensional anchor, adding the relay
  chain slot, is a candidate replacement.
