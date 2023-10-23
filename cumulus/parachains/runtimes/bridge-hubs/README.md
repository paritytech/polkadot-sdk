- [Bridge-hub Parachains](#bridge-hub-parachains)
  - [Requirements for local run/testing](#requirements-for-local-runtesting)
  - [How to test local Rococo <-> Wococo bridge](#how-to-test-local-rococo---wococo-bridge)
    - [Run Rococo/Wococo chains with zombienet](#run-rococowococo-chains-with-zombienet)
    - [Init bridge and run relayer between BridgeHubRococo and
      BridgeHubWococo](#init-bridge-and-run-relayer-between-bridgehubrococo-and-bridgehubwococo)
    - [Initialize configuration for transfer asset over bridge
      (ROCs/WOCs)](#initialize-configuration-for-transfer-asset-over-bridge-rocswocs)
    - [Send messages - transfer asset over bridge (ROCs/WOCs)](#send-messages---transfer-asset-over-bridge-rocswocs)
    - [Claim relayer's rewards on BridgeHubRococo and
      BridgeHubWococo](#claim-relayers-rewards-on-bridgehubrococo-and-bridgehubwococo)
  - [How to test local BridgeHubKusama/BridgeHubPolkadot](#how-to-test-local-bridgehubkusamabridgehubpolkadot)

# Bridge-hub Parachains

_BridgeHub(s)_ are **_system parachains_** that will house trustless bridges from the local ecosystem to others. The
current trustless bridges planned for the BridgeHub(s) are:
- `BridgeHubPolkadot` system parachain:
	1. Polkadot <-> Kusama bridge
	2. Polkadot <-> Ethereum bridge (Snowbridge)
- `BridgeHubKusama` system parachain:
	1. Kusama <-> Polkadot bridge
	2. Kusama <-> Ethereum bridge The high-level
	responsibilities of each bridge living on BridgeHub:
- sync finality proofs between relay chains (or equivalent)
- sync finality proofs between BridgeHub parachains
- pass (XCM) messages between different BridgeHub parachains

![](./docs/bridge-hub-parachain-design.jpg "Basic deployment setup")

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

# If you want to test Kusama/Polkadot bridge, we need "sudo pallet + fast-runtime",
# so we need to use sudofi in polkadot directory.
#
# Install sudofi: (skip if already installed)
# cd <somewhere-outside-polkadot-sdk-git-repo-dir>
# git clone https://github.com/paritytech/parachain-utils.git
# cd parachain-utils # -> this is <parachain-utils-git-repo-dir>
# cargo build --release --bin sudofi
#
# cd <polkadot-sdk-git-repo-dir>/polkadot
# <parachain-utils-git-repo-dir>/target/release/sudofi

cd <polkadot-sdk-git-repo-dir>
cargo build --release --features fast-runtime --bin polkadot
cp target/release/polkadot ~/local_bridge_testing/bin/polkadot

cargo build --release --features fast-runtime --bin polkadot-prepare-worker
cp target/release/polkadot-prepare-worker ~/local_bridge_testing/bin/polkadot-prepare-worker

cargo build --release --features fast-runtime --bin polkadot-execute-worker
cp target/release/polkadot-execute-worker ~/local_bridge_testing/bin/polkadot-execute-worker


---
# 3. Build substrate-relay binary
git clone https://github.com/paritytech/parity-bridges-common.git
cd parity-bridges-common

# checkout desired branch or use master:
# git checkout -b master --track origin/master
# `polkadot-staging` (recommended) is stabilized and compatible for Cumulus releases
# `master` is latest development
git checkout -b polkadot-staging --track origin/polkadot-staging

cargo build --release -p substrate-relay
cp target/release/substrate-relay ~/local_bridge_testing/bin/substrate-relay


---
# 4. Build cumulus polkadot-parachain binary
cd <polkadot-sdk-git-repo-dir>

cargo build --release -p polkadot-parachain-bin
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain
cp target/release/polkadot-parachain ~/local_bridge_testing/bin/polkadot-parachain-asset-hub
```


## How to test local Rococo <-> Wococo bridge

### Run Rococo/Wococo chains with zombienet

```
cd <polkadot-sdk-git-repo-dir>

# Rococo + BridgeHubRococo + AssetHub for Rococo (mirroring Kusama)
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_ROCOCO=~/local_bridge_testing/bin/polkadot-parachain-asset-hub \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./cumulus/zombienet/bridge-hubs/bridge_hub_rococo_local_network.toml
```

```
cd <polkadot-sdk-git-repo-dir>

# Wococo + BridgeHubWococo + AssetHub for Wococo (mirroring Polkadot)
POLKADOT_BINARY_PATH=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY_PATH=~/local_bridge_testing/bin/polkadot-parachain \
POLKADOT_PARACHAIN_BINARY_PATH_FOR_ASSET_HUB_WOCOCO=~/local_bridge_testing/bin/polkadot-parachain-asset-hub \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./cumulus/zombienet/bridge-hubs/bridge_hub_wococo_local_network.toml
```

### Init bridge and run relayer between BridgeHubRococo and BridgeHubWococo

**Accounts of BridgeHub parachains:**
- `Bob` is pallet owner of all bridge pallets

#### Run with script
```
cd <polkadot-sdk-git-repo-dir>

./cumulus/scripts/bridges_rococo_wococo.sh run-relay
```

**Check relay-chain headers relaying:**
- Rococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate - Pallet:
	**bridgeWococoGrandpa** - Keys: **bestFinalized()**
- Wococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate - Pallet:
	**bridgeRococoGrandpa** - Keys: **bestFinalized()**

**Check parachain headers relaying:**
- Rococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate - Pallet:
	**bridgeWococoParachain** - Keys: **parasInfo(None)**
- Wococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate - Pallet:
	**bridgeRococoParachain** - Keys: **parasInfo(None)**

### Initialize configuration for transfer asset over bridge (ROCs/WOCs)

This initialization does several things:
- creates `ForeignAssets` for wrappedROCs/wrappedWOCs
- drips SA for AssetHubRococo on AssetHubWococo (and vice versa) which holds reserved assets on source chains
```
cd <polkadot-sdk-git-repo-dir>

./cumulus/scripts/bridges_rococo_wococo.sh init-asset-hub-rococo-local
./cumulus/scripts/bridges_rococo_wococo.sh init-bridge-hub-rococo-local
./cumulus/scripts/bridges_rococo_wococo.sh init-asset-hub-wococo-local
./cumulus/scripts/bridges_rococo_wococo.sh init-bridge-hub-wococo-local
```

### Send messages - transfer asset over bridge (ROCs/WOCs)

Do (asset) transfers:
```
cd <polkadot-sdk-git-repo-dir>

# ROCs from Rococo's Asset Hub to Wococo's.
./cumulus/scripts/bridges_rococo_wococo.sh reserve-transfer-assets-from-asset-hub-rococo-local
```
```
cd <polkadot-sdk-git-repo-dir>

# WOCs from Wococo's Asset Hub to Rococo's.
./cumulus/scripts/bridges_rococo_wococo.sh reserve-transfer-assets-from-asset-hub-wococo-local
```

- open explorers: (see zombienets)
	- AssetHubRococo (see events `xcmpQueue.XcmpMessageSent`, `polkadotXcm.Attempted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9910#/explorer
	- BridgeHubRococo (see `bridgeRococoToWococoMessages.MessageAccepted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubWococo (see `bridgeWococoToRococoMessages.MessagesReceived`, `xcmpQueue.XcmpMessageSent`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer
	- AssetHubWococo (see `foreignAssets.Issued`, `xcmpQueue.Success`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9010#/explorer
	- BridgeHubRocococ (see `bridgeRococoToWococoMessages.MessagesDelivered`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer

### Claim relayer's rewards on BridgeHubRococo and BridgeHubWococo

**Accounts of BridgeHub parachains:**
- `//Charlie` is relayer account on BridgeHubRococo
- `//Charlie` is relayer account on BridgeHubWococo

```
cd <polkadot-sdk-git-repo-dir>

# Claim rewards on BridgeHubWococo:
./cumulus/scripts/bridges_rococo_wococo.sh claim-rewards-bridge-hub-rococo-local

# Claim rewards on BridgeHubWococo:
./cumulus/scripts/bridges_rococo_wococo.sh claim-rewards-bridge-hub-wococo-local
```

- open explorers: (see zombienets)
	- BridgeHubRococo (see 2x `bridgeRelayers.RewardPaid`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubWococo (see 2x `bridgeRelayers.RewardPaid`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer

## How to test local BridgeHubKusama/BridgeHubPolkadot

TODO: see `# !!! READ HERE` above
