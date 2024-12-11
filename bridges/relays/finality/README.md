# GRANDPA Finality Relay

The finality relay is able to work with different finality engines. In the modern Substrate world they are GRANDPA
and BEEFY. Let's talk about GRANDPA here, because BEEFY relay and bridge BEEFY pallet are in development.

In general, the relay works as follows: it connects to the source and target chain. The source chain must have the
[GRANDPA gadget](https://github.com/paritytech/finality-grandpa) running (so it can't be a parachain). The target
chain must have the [bridge GRANDPA pallet](../../modules/grandpa/) deployed at its runtime. The relay subscribes
to the GRANDPA finality notifications at the source chain and when the new justification is received, it is submitted
to the pallet at the target chain.

Apart from that, the relay is watching for every source header that is missing at target. If it finds the missing
mandatory header (header that is changing the current GRANDPA validators set), it submits the justification for
this header. The case when the source node can't return the mandatory justification is considered a fatal error,
because the pallet can't proceed without it.

More: [GRANDPA Finality Relay Sequence Diagram](../../docs/grandpa-finality-relay.html).

## How to Use the Finality Relay

The most important trait is the [`FinalitySyncPipeline`](./src/lib.rs), which defines the basic primitives of the
source chain (like block hash and number) and the type of finality proof (GRANDPA justification or MMR proof). Once
that is defined, there are two other traits - [`SourceClient`](./src/finality_loop.rs) and
[`TargetClient`](./src/finality_loop.rs).

The `SourceClient` represents the Substrate node client that connects to the source chain. The client needs to
be able to return the best finalized header number, finalized header and its finality proof and the stream of
finality proofs.

The `TargetClient` implementation must be able to craft finality delivery transaction and submit it to the target
node. The transaction is then tracked by the relay until it is mined and finalized.

The main entrypoint for the crate is the [`run` function](./src/finality_loop.rs), which takes source and target
clients and [`FinalitySyncParams`](./src/finality_loop.rs) parameters. The most important parameter is the
`only_mandatory_headers` - it is set to `true`, the relay will only submit mandatory headers. Since transactions
with mandatory headers are fee-free, the cost of running such relay is zero (in terms of fees). If a similar,
`only_free_headers` parameter, is set to `true`, then free headers (if configured in the runtime) are also
relayed.

## Finality Relay Metrics

Finality relay provides several metrics. Metrics names depend on names of source and target chains. The list below
shows metrics names for Rococo (source chain) to BridgeHubWestend (target chain) finality relay. For other
chains, simply change chain names. So the metrics are:

- `Rococo_to_BridgeHubWestend_Sync_best_source_block_number` - returns best finalized source chain (Rococo) block
  number, known to the relay.
  If relay is running in [on-demand mode](../bin-substrate/src/cli/relay_headers_and_messages/), the
  number may not match (it may be far behind) the actual best finalized number;

- `Rococo_to_BridgeHubWestend_Sync_best_source_at_target_block_number` - returns best finalized source chain (Rococo)
  block number that is known to the bridge GRANDPA pallet at the target chain.

- `Rococo_to_BridgeHubWestend_Sync_is_source_and_source_at_target_using_different_forks` - if this metrics is set
  to `1`, then  the best source chain header known to the target chain doesn't match the same-number-header
  at the source chain. It means that the GRANDPA validators set has crafted the duplicate justification
  and it has been submitted to the target chain.
  Normally (if majority of validators are honest and if you're running finality relay without large breaks)
  this shall not happen and the metric will have `0` value.

If relay operates properly, you should see that the `Rococo_to_BridgeHubWestend_Sync_best_source_at_target_block_number`
tries to reach the `Rococo_to_BridgeHubWestend_Sync_best_source_block_number`. And the latter one always increases.
