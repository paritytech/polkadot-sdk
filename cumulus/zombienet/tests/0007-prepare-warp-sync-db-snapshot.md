# Database snapshot guide

For this guide we will be taking a snapshot of a parachain and relay chain. Please note we are using a local chain here
`rococo_local_testnet` and `local_testnet`. Live chains will have different values

*Please ensure that the database is not in current use, i.e no nodes are writing to it*

# How to prepare database for a relaychain
To prepare snapshot for a relay chain we need to copy the database.

```
mkdir -p relaychain-snapshot/alice/data/chains/rococo_local_testnet/db/

cp -r chain-data/alice/data/chains/rococo_local_testnet/db/. relaychain-snapshot/alice/data/chains/rococo_local_testnet/db/

tar -C relaychain-snapshot/alice/ -czf relaychain.tgz data
```
# How to prepare database for a parachain

To prepare snapshot for a parachain we need to copy the database for both the collator node (parachain data) and
validator (relay data)

```
#Parachain data
mkdir -p parachain-snapshot/charlie/data/chains/local_testnet/db/

# Relay data
mkdir -p parachain-snapshot/charlie/relay-data/chains/rococo_local_testnet/db/

cp -r chain-data/charlie/data/chains/local_testnet/db/. parachain-snapshot/charlie/data/chains/local_testnet/db/

cp -r chain-data/charlie/relay-data/chains/rococo_local_testnet/db/. parachain-snapshot/charlie/relay-data/chains/rococo_local_testnet/db/

tar -C parachain-snapshot/charlie/ -czf parachain.tgz data relay-data
```

# Restoring a snapshot
Zombienet will automatically download the `*.tgz` file to the respective folder for a run. However you can also download
it manually, just ensure you extract the tar file in the correct directory, i.e. the root directory
`chain-data/charlie/`
