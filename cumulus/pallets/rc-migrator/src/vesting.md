# Pallet Vesting

Pallet vesting has one storage map to hold the vesting schedules and one storage value to track the
current version of the pallet. The version can be easily migrated, but for the schedules it is a bit difficult.

## Storage: Vesting

The vesting schedules are already measured in relay blocks, as can be seen
[here](https://github.com/polkadot-fellows/runtimes/blob/b613b54d94af5f4702533a56c6260651a14bdccb/system-parachains/asset-hubs/asset-hub-polkadot/src/lib.rs#L297).
This means that we can just integrate the existing schedules. The only possibly issue is when there
are lots of pre-existing schedules. The maximal number of schedules is 28; both on Relay and AH.  
We cannot use the merge functionality of the vesting pallet since that can be used as an attack
vector: anyone can send 28 vested transfers with very large unlock duration and low amount to force
all other schedules to adapt this long unlock period. This would reduce the rewards per block, which
is bad.  
For now, we are writing all colliding AH schedules into a storage item for manual inspection later.
It could still happen that unmalicious users will have more than 28 schedules, but as nobody has
used the vested transfers on AH yet.

Q: Maybe we should disable vested transfers with the next runtime upgrade on AH.

## Storage: StorageVersion

The vesting pallet is not using the proper FRAME version tracking; rather, it tracks its version in
the `StorageVersion` value. It does this incorrectly though, with Asset Hub reporting version 0
instead of 1. We ignore and correct this by writing 1 to the storage.


## User Impact

This affects users that have vesting schedules on the Relay chain or on Asset Hub. There exists a
risk that the number of total schedules exceeds 28, which means that they will not fit into the
storage anymore.  

We then prioritize the schedules from AH and pause and stash all schedules that do not fit (up to
28).
