# Parachains Finality Relay

The parachains finality relay works with two chains - source relay chain and target chain (which may be standalone
chain, relay chain or a parachain). The source chain must have the
[`paras` pallet](https://github.com/paritytech/polkadot/tree/master/runtime/parachains/src/paras) deployed at its
runtime. The target chain must have the [bridge parachains pallet](../../modules/parachains/) deployed at its runtime.

The relay is configured to submit heads of one or several parachains. It pokes source chain periodically and compares
parachain heads that are known to the source relay chain to heads at the target chain. If there are new heads,
the relay submits them to the target chain.

More: [Parachains Finality Relay Sequence Diagram](../../docs/parachains-finality-relay.html).

## How to Use the Parachains Finality Relay

There are only two traits that need to be implemented. The [`SourceChain`](./src/parachains_loop.rs) implementation
is supposed to connect to the source chain node. It must be able to read parachain heads from the `Heads` map of
the [`paras` pallet](https://github.com/paritytech/polkadot/tree/master/runtime/parachains/src/paras).
It also must create storage proofs of `Heads` map entries, when required.

The [`TargetChain`](./src/parachains_loop.rs) implementation connects to the target chain node. It must be able
to return the best known head of given parachain. When required, it must be able to craft and submit parachains
finality delivery transaction to the target node.

The main entrypoint for the crate is the [`run` function](./src/parachains_loop.rs), which takes source and target
clients and [`ParachainSyncParams`](./src/parachains_loop.rs) parameters. The most important parameter is the
`parachains` - it is the set of parachains, which relay tracks and updates. The other important parameter that
may affect the relay operational costs is the `strategy`. If it is set to `Any`, then the finality delivery
transaction is submitted if at least one of tracked parachain heads is updated. The other option is `All`. Then
the relay waits until all tracked parachain heads are updated and submits them all in a single finality delivery
transaction.

## Parachain Finality Relay Metrics

Every parachain in Polkadot is identified by the 32-bit number. All metrics, exposed by the parachains finality
relay have the `parachain` label, which is set to the parachain id. And the metrics are prefixed with the prefix,
that depends on the name of the source relay and target chains. The list below shows metrics names for
Rococo (source relay chain) to BridgeHubWestend (target chain) parachains finality relay. For other chains, simply
change chain names. So the metrics are:

- `Rococo_to_BridgeHubWestend_Parachains_best_parachain_block_number_at_source` - returns best known parachain block
  number, registered in the `paras` pallet at the source relay chain (Rococo in our example);

- `Rococo_to_BridgeHubWestend_Parachains_best_parachain_block_number_at_target` - returns best known parachain block
  number, registered in the bridge parachains pallet at the target chain (BridgeHubWestend in our example).

If relay operates properly, you should see that
the `Rococo_to_BridgeHubWestend_Parachains_best_parachain_block_number_at_target` tries to reach
the `Rococo_to_BridgeHubWestend_Parachains_best_parachain_block_number_at_source`.
And the latter one always increases.
