# Pallet Nomination Pools

The nomination pools pallet has 15 storage items of which 14 can be migrated without any
translation.

# Storage Values

All nine storage values are migrated as is in a single message.

# Storage Maps

The timestamp in is no longer translated and all the storage maps are migrated as they are.
The `BondedPools` map commission logic can be mapped as-is due to the use of the RelaychainDataProvider to get the relay block number. The Commission struct contains an absolute relay timestamp
[throttle_from](https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L737)
and the commission change
[min_delay](https://github.com/paritytech/polkadot-sdk/blob/bf20a9ee18f7215210bbbabf79e955c8c35b3360/substrate/frame/nomination-pools/src/lib.rs#L922).  

## User Impact

Impact here is negligible and only for pool operators - not members:
- Pool commission change rate (measured in blocks) could be decreased by one block.
- Pool operators may be able to change the commission rate one block later than anticipated. This is
  due to the nature or translating blocks of two different blockchains which does not yield
  unambiguous results.
