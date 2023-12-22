# Local Testnet

The E2E tests run against local deployments of the parachain, relayer, the ethereum execution layer (geth) and the ethereum consensus layer (lodestar).

## Requirements

The E2E stack can be run on any system that supports the [Nix](https://nixos.org/explore.html) package manager. This
includes Linux, MacOS, and Windows (WSL2). See the [main README's Development section](../../../README.md#Development) for
setup instructions. Ensure that you are in a Nix development shell for the remaining instructions.

## Setup

### Configure testnet

All required environment variables have reasonable defaults so normally there is no need to configure them. If necessary, you can override them with an `.envrc` file, using [.envrc-example](.envrc-example) as a template.

Once the `.envrc` has been created, let `direnv` load it automatically:

```bash
direnv allow
```

## Launch the testnet

Run the following script:

```bash
scripts/start-services.sh
```

Wait until the "Testnet has been initialized" message.

Go to polkadot-js and wait until the parachain has started producing blocks:
https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A11144#/explorer

You can see the relay chain here:
https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/explorer

Confirm the block number is > 2.

## Troubleshooting

The `start-services.sh` script writes the following logs:

- Rococo relaychain nodes: {rococo-alice,rococo-bob,rococo-charlie,9944}.log
- BridgeHub nodes: {bridgehub01,bridgehub02,11144}.log
- Statemine nodes: {statemine01,statemine02,12144}.log
- Relay services: {beefy,parachain,beacon}-relay.log
- Geth (execution client): /tmp/snowbridge/geth.log
- Lodestar (beacon client): /tmp/snowbridge/lodestar.log
