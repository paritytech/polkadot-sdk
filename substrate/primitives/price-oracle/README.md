# sp-price-oracle

Primitives shared by the price oracle node service (`sc-price-oracle`) and the price oracle
pallet (`pallet-price-oracle`): the types that cross the boundary between them, the inherent, and
the runtime APIs. Read the pallet's documentation first: it defines the vocabulary and explains
how prices are computed; this crate only carries the types.

- The crate root holds the report types: `PriceReport`, the prices a node reports, one `Quote`
  per pair, with the block height they are anchored to, and `SignedPriceReport`, the report with
  the signer's key and signature. Reports are signed with a domain prefix, so a report signature
  cannot serve as any other signature of the same key. `Settings` carries the rules a node
  follows: the accepted signers, the report window and the tick interval.
- `market` holds what a node fetches: `Market`, one pair on one venue with its `Query` list, and
  `Request`, an HTTPS request the node performs as given. These are the wire forms of the
  pallet's stored types.
- `inherents` defines the inherent identifier, the inherent data extension the pallet reads
  reports through, and the `InherentDataProvider` a block author fills.
- `runtime_api` declares `PriceOracleApi`, the five calls a node makes: `settings`,
  `latest_anchors`, `markets`, `parse` and `aggregate`.
