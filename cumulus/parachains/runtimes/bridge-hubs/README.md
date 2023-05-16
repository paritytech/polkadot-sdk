- [Bridge-hub Parachains](#bridge-hub-parachains)
	* [Requirements for local run/testing](#requirements-for-local-run-testing)
	* [How to test locally Rococo <-> Wococo bridge](#how-to-test-locally-rococo-----wococo-bridge)
		+ [Run chains (Rococo + BridgeHub, Wococo + BridgeHub) with zombienet](#run-chains--rococo---bridgehub--wococo---bridgehub--with-zombienet)
		+ [Run relayer (BridgeHubRococo, BridgeHubWococo)](#run-relayer--bridgehubrococo--bridgehubwococo-)
			- [Run with script (alternative 1)](#run-with-script--alternative-1-)
			- [Run with binary (alternative 2)](#run-with-binary--alternative-2-)
		+ [Send messages](#send-messages)
			- [Local zombienet run](#local-zombienet-run)
			- [Live Rockmine2 to Wockmint](#live-rockmine2-to-wockmint)
	* [How to test local BridgeHubKusama](#how-to-test-local-bridgehubkusama)
	* [How to test local BridgeHubPolkadot](#how-to-test-local-bridgehubpolkadot)

# Bridge-hub Parachains

_BridgeHub(s)_ are **_system parachains_** that will house trustless bridges from the local
ecosystem to others.
The current trustless bridges planned for the BridgeHub(s) are:
- `BridgeHubPolkadot` system parachain:
	1. Polkadot <-> Kusama bridge
	2. Polkadot <-> Ethereum bridge (Snowbridge)
- `BridgeHubKusama` system parachain:
	1. Kusama <-> Polkadot bridge
	2. Kusama <-> Ethereum bridge
	   The high-level responsibilities of each bridge living on BridgeHub:
- sync finality proofs between relay chains (or equivalent)
- sync finality proofs between BridgeHub parachains
- pass (XCM) messages between different BridgeHub parachains

![](./docs/bridge-hub-parachain-design.jpg "Basic deployment setup")

## Requirements for local run/testing

```
# Prepare empty directory for testing
mkdir -p ~/local_bridge_testing/bin
mkdir -p ~/local_bridge_testing/logs

# 1. Install zombienet
Go to: https://github.com/paritytech/zombienet/releases
Copy the apropriate binary (zombienet-linux) from the latest release to ~/local_bridge_testing/bin

# 2. Build polkadot binary
git clone https://github.com/paritytech/polkadot.git
cd polkadot

# if you want to test Kusama/Polkadot bridge, we need "sudo pallet + fast-runtime",
# so please, find the latest polkadot's repository branch `it/release-vX.Y.Z-fast-sudo`
# e.g:
# git checkout -b it/release-v0.9.42-fast-sudo --track origin/it/release-v0.9.42-fast-sudo

cargo build --release --features fast-runtime
cp target/release/polkadot ~/local_bridge_testing/bin/polkadot

# 3. Build cumulus polkadot-parachain binary
cd <cumulus-git-repo-dir>
# checkout desired branch or use master:
# git checkout -b bridge-hub-rococo-wococo --track origin/bridge-hub-rococo-wococo
git checkout -b master --track origin/master
cargo build --release --locked -p polkadot-parachain-bin
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain-mint

# 4. Build substrate-relay binary
git clone https://github.com/paritytech/parity-bridges-common.git
cd parity-bridges-common
cargo build --release -p substrate-relay
cp target/release/substrate-relay ~/local_bridge_testing/bin/substrate-relay

# (Optional) 5. Build polkadot-parachain-mint binary with statemine/westmint for moving assets
cd <cumulus-git-repo-dir>
git checkout -b bko-transfer-asset-via-bridge --track origin/bko-transfer-asset-via-bridge
cargo build --release --locked -p polkadot-parachain-bin
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain-mint
```

## How to test locally Rococo <-> Wococo bridge

### Run chains (Rococo + BridgeHub, Wococo + BridgeHub) with zombienet

```
# Rococo + BridgeHubRococo + Rockmine (mirroring Kusama)
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
POLKADOT_PARACHAIN_BINARY_PATH_FOR_ROCKMINE=~/local_bridge_testing/bin/polkadot-parachain-mint \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./zombienet/bridge-hubs/bridge_hub_rococo_local_network.toml
```

```
# Wococo + BridgeHubWococo + Wockmint (mirroring Polkadot)
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
POLKADOT_PARACHAIN_BINARY_PATH_FOR_WOCKMINT=~/local_bridge_testing/bin/polkadot-parachain-mint \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./zombienet/bridge-hubs/bridge_hub_wococo_local_network.toml
```

### Run relayer (BridgeHubRococo, BridgeHubWococo)

**Accounts of BridgeHub parachains:**
- `Bob` is pallet owner of all bridge pallets

#### Run with script (alternative 1)
```
cd <cumulus-git-repo-dir>
./scripts/bridges_rococo_wococo.sh run-relay
```

#### Run with binary (alternative 2)
Need to wait for parachain activation (start producing blocks), then run:

```
# 1. Init bridges:

# Rococo -> Wococo
RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
	~/local_bridge_testing/bin/substrate-relay init-bridge rococo-to-bridge-hub-wococo \
	--source-host localhost \
	--source-port 9942 \
	--source-version-mode Auto \
	--target-host localhost \
	--target-port 8945 \
	--target-version-mode Auto \
	--target-signer //Bob

# Wococo -> Rococo
RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
	~/local_bridge_testing/bin/substrate-relay init-bridge wococo-to-bridge-hub-rococo \
	--source-host localhost \
	--source-port 9945 \
	--source-version-mode Auto \
	--target-host localhost \
	--target-port 8943 \
	--target-version-mode Auto \
	--target-signer //Bob

# 2. Relay relay-chain headers, parachain headers and messages**
RUST_LOG=runtime=trace,rpc=trace,bridge=trace \
    ~/local_bridge_testing/bin/substrate-relay relay-headers-and-messages bridge-hub-rococo-bridge-hub-wococo \
    --rococo-host localhost \
    --rococo-port 9942 \
    --rococo-version-mode Auto \
    --bridge-hub-rococo-host localhost \
    --bridge-hub-rococo-port 8943 \
    --bridge-hub-rococo-version-mode Auto \
    --bridge-hub-rococo-signer //Charlie \
    --wococo-headers-to-bridge-hub-rococo-signer //Bob \
    --wococo-parachains-to-bridge-hub-rococo-signer //Bob \
    --bridge-hub-rococo-transactions-mortality 4 \
    --wococo-host localhost \
    --wococo-port 9945 \
    --wococo-version-mode Auto \
    --bridge-hub-wococo-host localhost \
    --bridge-hub-wococo-port 8945 \
    --bridge-hub-wococo-version-mode Auto \
    --bridge-hub-wococo-signer //Charlie \
    --rococo-headers-to-bridge-hub-wococo-signer //Bob \
    --rococo-parachains-to-bridge-hub-wococo-signer //Bob \
    --bridge-hub-wococo-transactions-mortality 4 \
    --lane 00000001
```

**Check relay-chain headers relaying:**
- Rococo parachain:
	- https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate
	- Pallet: **bridgeWococoGrandpa**
	- Keys: **bestFinalized()**
- Wococo parachain:
	- https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate
	- Pallet: **bridgeRococoGrandpa**
	- Keys: **bestFinalized()**

**Check parachain headers relaying:**
- Rococo parachain:
	- https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate
	- Pallet: **bridgeWococoParachain**
	- Keys: **bestParaHeads()**
- Wococo parachain:
	- https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate
	- Pallet: **bridgeRococoParachain**
	- Keys: **bestParaHeads()**

### Send messages

#### Local zombienet run

1. allow bridge transfer on statemine/westmint (governance-like):
   ```
   ./scripts/bridges_rococo_wococo.sh allow-transfers-local
   ```

2. do (asset) transfer from statemine to westmint
   ```
   ./scripts/bridges_rococo_wococo.sh transfer-asset-from-statemine-local
   ```

3. do (ping) transfer from statemine to westmint
   ```
   ./scripts/bridges_rococo_wococo.sh ping-via-bridge-from-statemine-local
   ```

- open explorers: (see zombienets)
	- Statemine (see events `xcmpQueue.XcmpMessageSent`, `bridgeTransfer.ReserveAssetsDeposited`, `bridgeTransfer.TransferInitiated`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9910#/explorer
	- BridgeHubRococo (see `bridgeWococoMessages.MessageAccepted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubWococo (see `bridgeRococoMessages.MessagesReceived`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer
	- Westmint (see `xcmpQueue.Success` for `transfer-asset` and `xcmpQueue.Fail` for `ping-via-bridge`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9010#/explorer
    - BridgeHubRococo (see `bridgeWococoMessages.MessagesDelivered`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer

#### Live Rockmine2 to Wockmint
- uses account seed on Live Rococo:Rockmine2
  ```
  cd <cumulus-git-repo-dir>

  ./scripts/bridges_rococo_wococo.sh transfer-asset-from-statemine-rococo
  or
  ./scripts/bridges_rococo_wococo.sh ping-via-bridge-from-statemine-rococo
  ```

- open explorers:
	- Rockmine2 (see events `xcmpQueue.XcmpMessageSent`, `bridgeTransfer.ReserveAssetsDeposited`, `bridgeTransfer.TransferInitiated`) https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fws-rococo-rockmine2-collator-node-0.parity-testnet.parity.io#/explorer
	- BridgeHubRococo (see `bridgeWococoMessages.MessageAccepted`) https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frococo-bridge-hub-rpc.polkadot.io#/explorer
	- BridgeHubWococo (see `bridgeRococoMessages.MessagesReceived`) https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fwococo-bridge-hub-rpc.polkadot.io#/explorer
	- Wockmint (see `xcmpQueue.Success` for `transfer-asset` and `xcmpQueue.Fail` for `ping-via-bridge`) https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fws-wococo-wockmint-collator-node-0.parity-testnet.parity.io#/explorer
	- BridgeHubRococo (see `bridgeWococoMessages.MessagesDelivered`)

## How to test local BridgeHubKusama
```
cd <base-cumulus-repo-directory>
cargo build --release -p polkadot-parachain-bin

# script expect to have pre-built polkadot binary on the path: ../polkadot/target/release/polkadot
# if using `kusama-local` / `polkadot-local`, build polkadot with `--features fast-runtime`

# BridgeHubKusama
zombienet-linux --provider native spawn ./zombienet/examples/bridge_hub_kusama_local_network.toml

or

# BridgeHubPolkadot
zombienet-linux --provider native spawn ./zombienet/examples/bridge_hub_polkadot_local_network.toml
```

## How to test local BridgeHubPolkadot
TODO: from master
