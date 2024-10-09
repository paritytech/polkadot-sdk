## Start the node

Start the kitchensink node

```bash
RUST_LOG="error,evm=debug,sc_rpc_server=info,runtime::revive=debug" cargo run --bin substrate-node -- --dev
```

## Start the RPC server

This command starts the Ethereum JSON-RPC server, by default it runs on `localhost:9090`

```bash
RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc --features dev
```

## Send a transaction using provided examples

You can run one of the examples from the `examples` directory to send a transaction to the node.

```bash
RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc --features example --example deploy
```

## Interact with the node using MetaMask & Ether.js

Start the Ether.js demo server, and open the demo Ether.js web page at `http://localhost:3000`

```bash
cd substrate/frame/revive/rpc/demo && yarn && yarn dev
```

### Configure MetaMask

You can use the following instructions to setup [MetaMask](https://metamask.io) with the local chain.

> Note: When you interact with MetaMask and restart the chain, you need to clear the activity tab (Settings > Advanced > Clear activity tab data)
> See [here](https://support.metamask.io/managing-my-wallet/resetting-deleting-and-restoring/how-to-clear-your-account-activity-reset-account) for more info on how to reset the account activity.

#### Add a new network

To interact with the local chain, you need to add a new network in [MetaMask](https://metamask.io).
See [here](https://support.metamask.io/networks-and-sidechains/managing-networks/how-to-add-a-custom-network-rpc/#adding-a-network-manually) for more info on how to add a custom network.

Make sure the node and the rpc server are started, and use the following settings to configure the network (MetaMask > Networks > Add a network manually):

- Network name: KitchenSink
- RPC URL: <http://localhost:9090>
- Chain ID: 420420420
- Currency Symbol: `DEV`

#### Import Dev account

You will need to import the following account that is endowed with some balance at genesis to interact with the chain.
See [here](https://support.metamask.io/managing-my-wallet/accounts-and-addresses/how-to-import-an-account/) for more info on how to import an account.

- Account: `0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac`
- Private Key: `5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133`

