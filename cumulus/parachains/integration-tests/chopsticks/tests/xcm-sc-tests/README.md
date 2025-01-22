## Test XCM unstable APIs in pallet_revive using Chopsticks

Override Westend Asset Hub runtime, allowing unstable APIs to be used by changing pallet_revive's [config](https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/parachains/runtimes/assets/asset-hub-westend/src/lib.rs#L1075)

```rs=1075
type UnsafeUnstableInterface = ConstBool<true>;
```

Build the new wasm

```console
$ cargo build --release -p asset-hub-westend-runtime
```

Run this command under the `chopsticks` folder to run the node

```console
$ RUST_LOG="error,evm=debug,sc_rpc_server=info,runtime::revive=debug" bunx @acala-network/chopsticks@latest xcm -p configs/westend-asset-hub-override.yaml
```

Run the Ethereum JSON-RPC server and connect it to the node running on localhost:8000

```console
$ RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc -- --dev --node-rpc-url ws://localhost:8000
```

---

In order to compile the Rust contracts, under the `xcm-sc-tests/contracts` folder (make sure to have `polkatool v0.19` installed)

```console
$ cargo build
$ ./build.sh
```

For tests using PAPI, you'd have to add the descriptors

```console
$ bun papi add wnd_ah -n westend2_asset_hub
```

Run tests from the `xcm-sc-tests` directory, e.g.:

```console
$ bun ./index.ts -k ${YOUR_PRIVATE_KEY}
```

Deploy the Rust contract with `deployRustContract.ts` and then deploy the Solidity contract with `deploySolidityContract.ts`. You can then call `xcmExecuteFromRustContract.ts` or `xcmExecuteFromSolidityContract.ts`
