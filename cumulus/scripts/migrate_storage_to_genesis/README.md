# Migrate storage to genesis state
Script to query storage under a particular key and add it to a raw chain_spec json file as genesis state.

# How to use
The storage to be quired can be selected by either providing a pallet name (e.g. `Identity`) or a key in hex format (e.g. 
`0x2aeddc77fe58c98d50bd37f1b90840f9`)
```
yarn migrate -c <wss_chain_enpoint> -f <chain_spec_raw_to_edit.json> -p <pallet_name> -k <storage_key>
```
