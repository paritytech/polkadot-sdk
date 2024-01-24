# Migrate storage to genesis state
Script to query storage under a particular key and add it to a raw chain_spec json file as genesis state.

# How to use
The storage to be queried can be selected by either providing a pallet name or a key in hex format.
```
yarn migrate -c <wss_chain_enpoint> -f <chain_spec_raw_to_edit.json> -p <pallet_name> -k <storage_key> -m <js_migration_function_file>
```

Example for migrating `Identity` pallet from Polkadot:
```
yarn migrate -c wss://rpc.polkadot.io -f new_genesis_raw.json -p Identity -m ./identityMigration.js
```
