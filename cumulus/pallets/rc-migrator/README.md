# Asset Hub Migration

## Stages

This explanation goes from general to specific. The basic gestalt looks like this:

1. Preparation
   1. Runtime Upgrade
   2. Scheduling
2. Migration
3. Cleanup

### Preparation

#### Runtime Upgrade

There will be a runtime upgrade that contains all the code that is needed for the migration. This
code will be in *passive* form and do nothing on its own. It is there on standby for the next step.

#### Scheduling

The Polkadot Technical Fellowship will determine the proper time point to start the migration. They
fix this by either block number or on an era and submit this to the Relay Chain.

### Migration

The migration will begin to run from the fixed block number and emit the following events to notify of this:
- `pallet_rc_migrator::AssetHubMigrationStarted` on the Relay Chain
- `pallet_ah_migrator::AssetHubMigrationStarted` on the Asset Hub

You can listen for these events to know whether the migration is ongoing.

The first thing the migration does, is to lock functionality on the Relay and Asset Hub. the locking
happens to ensure that no changes interfere with the migration.

Once it is done, two more events are emitted, respectively:

- `pallet_rc_migrator::AssetHubMigrationFinished` on the Relay Chain
- `pallet_ah_migrator::AssetHubMigrationFinished` on the Asset Hub

### Cleanup

This phase will unlock all functionality on Asset Hub.
