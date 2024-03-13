- [Bridge-hub Parachains](#bridge-hub-parachains)
  - [Requirements for local run/testing](#requirements-for-local-runtesting)
  - [How to test local Rococo <-> Westend bridge](#how-to-test-local-rococo---westend-bridge)
	  - [Run Rococo/Westend chains with zombienet](#run-rococowestend-chains-with-zombienet)
	  - [Init bridge and run relayer between BridgeHubRococo and
		BridgeHubWestend](#init-bridge-and-run-relayer-between-bridgehubrococo-and-bridgehubwestend)
	  - [Initialize configuration for transfer asset over bridge
		(ROCs/WNDs)](#initialize-configuration-for-transfer-asset-over-bridge-rocswnds)
	  - [Send messages - transfer asset over bridge (ROCs/WNDs)](#send-messages---transfer-asset-over-bridge-rocswnds)
	  - [Claim relayer's rewards on BridgeHubRococo and
		BridgeHubWestend](#claim-relayers-rewards-on-bridgehubrococo-and-bridgehubwestend)
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

We need polkadot binary with "fast-runtime" feature:

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

## How to test local Rococo <-> Westend bridge

### Run Rococo/Westend chains with zombienet

```
cd <polkadot-sdk-git-repo-dir>

# Rococo + BridgeHubRococo + AssetHub for Rococo (mirroring Kusama)
POLKADOT_BINARY=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY=~/local_bridge_testing/bin/polkadot-parachain \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./bridges/testing/environments/rococo-westend/bridge_hub_rococo_local_network.toml
```

```
cd <polkadot-sdk-git-repo-dir>

# Westend + BridgeHubWestend + AssetHub for Westend (mirroring Polkadot)
POLKADOT_BINARY=~/local_bridge_testing/bin/polkadot \
POLKADOT_PARACHAIN_BINARY=~/local_bridge_testing/bin/polkadot-parachain \
	~/local_bridge_testing/bin/zombienet-linux --provider native spawn ./bridges/testing/environments/rococo-westend/bridge_hub_westend_local_network.toml
```

### Init bridge and run relayer between BridgeHubRococo and BridgeHubWestend

**Accounts of BridgeHub parachains:**
- `Bob` is pallet owner of all bridge pallets

#### Run with script
```
cd <polkadot-sdk-git-repo-dir>

./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh run-relay
```

**Check relay-chain headers relaying:**
- Rococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate - Pallet:
  **bridgeWestendGrandpa** - Keys: **bestFinalized()**
- Westend parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate - Pallet:
  **bridgeRococoGrandpa** - Keys: **bestFinalized()**

**Check parachain headers relaying:**
- Rococo parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8943#/chainstate - Pallet:
  **bridgeWestendParachains** - Keys: **parasInfo(None)**
- Westend parachain: - https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A8945#/chainstate - Pallet:
  **bridgeRococoParachains** - Keys: **parasInfo(None)**

### Initialize configuration for transfer asset over bridge (ROCs/WNDs)

This initialization does several things:
- creates `ForeignAssets` for wrappedROCs/wrappedWNDs
- drips SA for AssetHubRococo on AssetHubWestend (and vice versa) which holds reserved assets on source chains
```
cd <polkadot-sdk-git-repo-dir>

./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh init-asset-hub-rococo-local
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh init-bridge-hub-rococo-local
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh init-asset-hub-westend-local
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh init-bridge-hub-westend-local
```

### Send messages - transfer asset over bridge (ROCs/WNDs)

Do reserve-backed transfers:
```
cd <polkadot-sdk-git-repo-dir>

# ROCs from Rococo's Asset Hub to Westend's.
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh reserve-transfer-assets-from-asset-hub-rococo-local
```
```
cd <polkadot-sdk-git-repo-dir>

# WNDs from Westend's Asset Hub to Rococo's.
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh reserve-transfer-assets-from-asset-hub-westend-local
```

- open explorers: (see zombienets)
	- AssetHubRococo (see events `xcmpQueue.XcmpMessageSent`, `polkadotXcm.Attempted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9910#/explorer
	- BridgeHubRococo (see `bridgeWestendMessages.MessageAccepted`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubWestend (see `bridgeRococoMessages.MessagesReceived`, `xcmpQueue.XcmpMessageSent`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer
	- AssetHubWestend (see `foreignAssets.Issued`, `xcmpQueue.Success`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9010#/explorer
	- BridgeHubRocococ (see `bridgeWestendMessages.MessagesDelivered`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer

Do reserve withdraw transfers: (when previous is finished)
```
cd <polkadot-sdk-git-repo-dir>

# wrappedWNDs from Rococo's Asset Hub to Westend's.
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh withdraw-reserve-assets-from-asset-hub-rococo-local
```
```
cd <polkadot-sdk-git-repo-dir>

# wrappedROCs from Westend's Asset Hub to Rococo's.
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh withdraw-reserve-assets-from-asset-hub-westend-local
```

### Claim relayer's rewards on BridgeHubRococo and BridgeHubWestend

**Accounts of BridgeHub parachains:**
- `//Charlie` is relayer account on BridgeHubRococo
- `//Charlie` is relayer account on BridgeHubWestend

```
cd <polkadot-sdk-git-repo-dir>

# Claim rewards on BridgeHubWestend:
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh claim-rewards-bridge-hub-rococo-local

# Claim rewards on BridgeHubWestend:
./bridges/testing/environments/rococo-westend/bridges_rococo_westend.sh claim-rewards-bridge-hub-westend-local
```

- open explorers: (see zombienets)
	- BridgeHubRococo (see 2x `bridgeRelayers.RewardPaid`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8943#/explorer
	- BridgeHubWestend (see 2x `bridgeRelayers.RewardPaid`) https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8945#/explorer

## How to test local BridgeHubKusama/BridgeHubPolkadot

TODO: see `# !!! READ HERE` above
