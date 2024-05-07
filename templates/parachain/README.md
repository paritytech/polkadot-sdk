## Staking Testnet

1. **Build the `polkadot` binary (relay-chain node)**
```
$ cargo build -p polkadot
```

2. **Build the `staking-node` binary**
```
$ cargo build -p staking-node
```

3. (optional) **Change and build the chainspec** 
```
# chainspec changes at `/polkadot-sdk/templates/parachain/node/src/chain_spec.rs`
$ staking-node build-spec --disable-default-bootnode > chain-specs/staking.spec

# (or use the Zombienet's `chain_spec_command` which will compile and generate the chainspec on the fly before spinning up the collator node)
```

4. **Edit the zombienet config `./staking_dev_network.toml`**

5. **Run Zombienet**
```
$ zombienet -l text spawn --provider native staking_dev_network.toml
```
