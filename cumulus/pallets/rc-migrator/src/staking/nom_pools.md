# Pallet Nomination Pools

The nomination pools pallet has 15 storage items of which 14 can be migrated without any
translation.

# Storage Values

All nine storage values are migrated as is in a single message.

# Storage Maps

The `BondedPools` map needs translation for its commission logic. The Commission struct contains an
absolute relay timestamp
[throttle_from](https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L737)
and the commission change
[min_delay](https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L922).  
The translation for both happens upon arrival on the Asset Hub. Ideally, it would be done on the
Relay chain side since we have more compute power there but it is not possible since the timestamp
translation depends on the Relay number upon arrival on Asset Hub.

The timestamp is translated in
[rc_to_ah_timestamp](https://github.com/polkadot-fellows/runtimes/blob/5af776e1443b5e7eb17b6e9d87ef40311afaf6f9/pallets/ah-migrator/src/staking/nom_pools.rs#L127)
and the `min_delay` is directly passed to `RcToAhDelay`.

The other five storage maps are migrated as it.

## User Impact

Impact here is negligible and only for pool operators - not members:
- Pool commission change rate (measured in blocks) could be decreased by one block.
- Pool operators may be able to change the commission rate one block later than anticipated. This is
  due to the nature or translating blocks of two different blockchains which does not yield
  unambiguous results.
