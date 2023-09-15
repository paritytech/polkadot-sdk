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

# checkout the `sv-bulletin-chain-bridge` branch:
git checkout -b sv-bulletin-chain-bridge --track origin/sv-bulletin-chain-bridge

cargo build -p polkadot --release --features fast-runtime --features polkadot-native
cp target/release/polkadot ~/local_bridge_testing/bin/polkadot
cp target/release/polkadot-execute-worker ~/local_bridge_testing/bin
cp target/release/polkadot-prepare-worker ~/local_bridge_testing/bin

---

# 3. Build substrate-relay binary

git clone https://github.com/paritytech/parity-bridges-common.git
cd parity-bridges-common

# checkout `polkadot-v.1.0.0-audited` branch:

git checkout -b polkadot-v.1.0.0-audited --track origin/polkadot-v.1.0.0-audited

cargo build --release -p substrate-relay
cp target/release/substrate-relay ~/local_bridge_testing/bin/substrate-relay

---

# 4. Build cumulus polkadot-parachain binary

git clone https://github.com/paritytech/polkadot-sdk
cd polkadot-sdk

# checkout the `sv-bulletin-chain-bridge` branch:
git checkout -b sv-bulletin-chain-bridge --track origin/sv-bulletin-chain-bridge

cargo build --release --locked --bin polkadot-parachain
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain

---

# 5. Build polkadot bulletin chain binary

git clone https://github.com/svyatonik/polkadot-bulletin-chain
cd polkadot-bulletin-chain

# checkout the `add-bridge-pallets` branch:
git checkout -b add-bridge-pallets --track origin/add-bridge-pallets

cargo build --release --locked
cp target/release/polkadot-bulletin-chain ~/local_bridge_testing/bin/polkadot-bulletin-chain

```

## How to test it locally

Check [requirements](#requirements-for-local-runtesting) for "sudo pallet + fast-runtime".

### 1. Run chains (Polkadot + BridgeHub, Bulletin Chain) with zombienet

Assuming that the sources root directory is the `~/dev`, following scripts must be called from the
`~/local_bridge_testing/` folder.

```
# Polkadot + BridgeHubPolkadot
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ~/dev/polkadot-sdk/cumulus/zombienet/bridge-hub-polkadot-and-bulletin/bridge_hub_polkadot_local_network.toml
```

```
# Polkadot Bulletin Chain

~/local_bridge_testing/bin/zombienet-linux --provider native spawn ~/dev/polkadot-bulletin-chain/zombienet/config.toml

```

### 2. Run relay

```
~/dev/polkadot-sdk/cumulus/scripts/bridges_polkadot_bulletin.sh run-relay
```


### 4. Open explorers

- Polkadot: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9945#/parachains
- Polkadot Bridge Hub: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/rpc
- Polkadot Bulletin: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9942#/rpc
















### 2. Init bridge and run relayer (BridgeHubKusama, BridgeHubPolkadot)

```
cd <cumulus-git-repo-dir>
./scripts/bridges_kusama_polkadot.sh run-relay
```

### 3. Initialize transfer asset over bridge (DOTs/KSMs)

This initialization does several things:
- creates `ForeignAssets` for wrappedDOTs/wrappedKSMs
- drips SA for AssetHubKusama on AssetHubPolkadot (and vice versa) which holds reserved assets on source chains
```
./scripts/bridges_kusama_polkadot.sh init-asset-hub-kusama-local
./scripts/bridges_kusama_polkadot.sh init-bridge-hub-kusama-local
./scripts/bridges_kusama_polkadot.sh init-asset-hub-polkadot-local
./scripts/bridges_kusama_polkadot.sh init-bridge-hub-polkadot-local
```

### 4. Send messages - transfer asset over bridge (DOTs/KSMs)

Do (asset) transfers:
```
# KSMs from Kusama's Asset Hub to Polkadot's.
./scripts/bridges_kusama_polkadot.sh reserve-transfer-assets-from-asset-hub-kusama-local
```
```
# DOTs from Polkadot's Asset Hub to Kusama's.
./scripts/bridges_kusama_polkadot.sh reserve-transfer-assets-from-asset-hub-polkadot-local
```

- open explorers: (see zombienets)
	- AssetHubKusama (see events `xcmpQueue.XcmpMessageSent`, `polkadotXcm.Attempted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9910#/explorer
	- BridgeHubKusama (see `bridgePolkadotMessages.MessageAccepted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubPolkadot (see `bridgeKusamaMessages.MessagesReceived`, `xcmpQueue.XcmpMessageSent`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer
	- AssetHubPolkadot (see `foreignAssets.Issued`, `xcmpQueue.Success`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9010#/explorer
	- BridgeHubKusama (see `bridgePolkadotMessages.MessagesDelivered`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
