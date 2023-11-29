## Requirements for local run/testing

```
# Prepare empty directory for testing
mkdir -p ~/local_bridge_testing/bin
mkdir -p ~/local_bridge_testing/logs

---

# 1. Install zombienet
Go to: https://github.com/paritytech/zombienet/releases
Copy the apropriate binary (zombienet-linux) from the latest release to ~/local_bridge_testing/bin

---

# 2. Build polkadot binary
git clone https://github.com/paritytech/polkadot-sdk
cd polkadot-sdk

# checkout the `sv-rococo-bulletin-bridge` branch:
git checkout -b sv-rococo-bulletin-bridge --track origin/sv-rococo-bulletin-bridge

cargo build -p polkadot --release --features fast-runtime
cp target/release/polkadot ~/local_bridge_testing/bin/polkadot
cp target/release/polkadot-execute-worker ~/local_bridge_testing/bin
cp target/release/polkadot-prepare-worker ~/local_bridge_testing/bin

---

# 3. Build substrate-relay binary

git clone https://github.com/paritytech/parity-bridges-common.git
cd parity-bridges-common

# checkout `rococo-bulletin-bridge` branch:

git checkout -b rococo-bulletin-bridge --track origin/rococo-bulletin-bridge

cargo build --release -p substrate-relay
cp target/release/substrate-relay ~/local_bridge_testing/bin/substrate-relay

---

# 4. Build cumulus polkadot-parachain binary

git clone https://github.com/paritytech/polkadot-sdk
cd polkadot-sdk

# checkout the `sv-rococo-bulletin-bridge` branch:
git checkout -b sv-rococo-bulletin-bridge --track origin/sv-rococo-bulletin-bridge

cargo build --release --locked -p polkadot-parachain-bin
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain

---

# 5. Build polkadot bulletin chain binary

git clone https://github.com/zdave-parity/polkadot-bulletin-chain.git
cd polkadot-bulletin-chain

# checkout the `add-bridge-dispatcher` branch:
git checkout -b add-bridge-dispatcher --track origin/add-bridge-dispatcher

cargo build -p polkadot-bulletin-chain --release --features rococo
cp target/release/polkadot-bulletin-chain ~/local_bridge_testing/bin/polkadot-bulletin-chain

```

## How to test it locally

Check [requirements](#requirements-for-local-runtesting) for "sudo pallet + fast-runtime".

### 1. Run chains (Rococo + BridgeHub + PeopleRococo, Bulletin Chain) with zombienet

Assuming that the sources root directory is the `~/dev`, following scripts must be called from the
`~/local_bridge_testing/` folder.

```
# Rococo + BridgeHubRococo + 
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ~/dev/polkadot-sdk/cumulus/zombienet/bridge-hubs/bridge_hub_rococo_local_with_bulletin_network.toml
```

```
# Polkadot Bulletin Chain
# (you may need to change path to polkadot-bulletin-chain binary in the config.toml)

~/local_bridge_testing/bin/zombienet-linux --provider native spawn ~/dev/polkadot-bulletin-chain/zombienet/config.toml

```

### 2. Run relay

```
~/dev/polkadot-sdk/cumulus/scripts/bridges_polkadot_bulletin.sh init-people-rococo-local
~/dev/polkadot-sdk/cumulus/scripts/bridges_polkadot_bulletin.sh run-relay
```

### 3. Open explorers

- Rococo: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9942#/explorer
- Rococo Bridge Hub: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
- Rococo People: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9910#/explorer
- Rococo Bulletin: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:10000#/explorer

Polkadot BH is currently configured to send messages to Polkadot Bulletin chain at every block.