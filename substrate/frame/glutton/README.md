# WARNING

**DO NOT USE ON VALUE-BEARING CHAINS. THIS PALLET IS ONLY INTENDED FOR TESTING USAGE.**

# Glutton Pallet

The `Glutton` pallet gets the name from its property to consume vast amounts of resources. It can be used to push
para-chains and their relay-chains to the limits. This is good for testing out theoretical limits in a practical way.

The `Glutton` can be set to consume a fraction of the available block length and unused weight of a chain. It
accomplishes this by filling the block length up to a ration and utilizing the `on_idle` hook to consume a
specific ration of the remaining weight. The rations can be set via `set_compute`, `set_storage` and `set_block_length`.
Initially the `Glutton` needs to be initialized once with `initialize_pallet`.
