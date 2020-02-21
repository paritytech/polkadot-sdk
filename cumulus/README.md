# Cumulus

A set of tools for writing [Polkadot](https://github.com/paritytech/polkadot) parachains that are based on [Substrate](https://github.com/paritytech/substrate).

It's easy to write blockchains using Substrate, and the overhead of writing parachains' distribution, p2p, database, and synchronization layers is generally high and should be reusable. This project aims to make it easy to write parachains for Polkadot by leveraging the power of Substrate.

Cumulus clouds are shaped sort of like dots and are up in the air, like this project (as it is an initial prototype -- expect a rename when it gets cooler.)

## cumulus-consensus

For now, this is only project contained in this repo. *cumulus-consensus* is a consensus engine for Substrate which follows a Polkadot relay chain. This will run a Polkadot node internally, and dictate to the client and synchronization algorithms which chain to follow, finalize, and treat as best.

## cumulus-runtime

A planned wrapper around substrate runtimes to turn them into parachain validation code and to provide proof-generation routines.

## cumulus-collator

A planned Polkadot collator for the parachain.

## Running a collator

1. Checkout polkadot at `96f5dc510ef770fd5c5ab57a90565bb5819bbbea`.

2. Run `Alice` and `Bob`:

	`cargo run --release -- --chain=${CUMULUS_REPO}/test/parachain/res/polkadot_chainspec.json --base-path=cumulus_relay_chain_node_0 --alice`

	`cargo run --release -- --chain=${CUMULUS_REPO}/test/parachain/res/polkadot_chainspec.json --base-path=cumulus_relay_chain_node_1 --bob --port 50666`

    Where `CUMULUS_REPO` is the path to the checkout of Cumulus.

3. Switch back to this repository and generate the parachain genesis state:

	`cargo run --release -p cumulus-test-parachain-collator -- export-genesis-state genesis-state`

4. Run the collator:

	`cargo run --release -p cumulus-test-parachain-collator -- --base-path cumulus_collator_path -- --bootnodes=/ip4/127.0.0.1/tcp/30333/p2p/PEER_ID_${NAME} --bootnodes=/ip4/127.0.0.1/tcp/50666/p2p/PEER_ID_${NAME}`

	`PEER_ID_${NAME}` needs to be replaced with the peer id of the polkadot validator that uses `${NAME}`
	as authority. The `--` after `--base-path cumulus_collator_path` is important, it tells the CLI to pass these arguments
	to the relay chain node that is running inside of the collator.

5. Open `https://polkadot.js.org/apps/#/sudo` and register the parachain by calling `Registrar > RegisterPara`

	`id`: `100`

	`ParaInfo`: `Always`

	`code`: `CUMULUS_REPO/target/release/wbuild/cumulus-test-parachain-runtime/cumulus_test_parachain_runtime.compact.wasm`

	`initial_head_data`: Use the file you generated in step 3. (name: genesis-state)

	Now your parachain should be registered and the collator should start building blocks and sending
	them to the relay chain.

6. Now the `collator` should build blocks and the relay-chain should include them. You can check that the `parachain-header` for parachain `100` is changing.

### Running the collator automatically

To simplify the above process, you can run steps 1-5 above automatically:

```sh
export BRANCH=96f5dc510ef770fd5c5ab57a90565bb5819bbbea
scripts/build_polkadot.sh
scripts/run_collator.sh
```

This will churn for several minutes, but should end with docker reporting that several containers have successfully been brought up.

To run step 6, first set up an alias which gives you quick access to the polkadot-js CLI:

```sh
docker build -f docker/parachain-registrar.dockerfile --target pjs -t parachain-registrar:pjs .
alias pjs='docker run --rm --net cumulus_testing_net parachain-registrar:pjs --ws ws://172.28.1.1:9944'
```

Those steps should complete very quickly. At that point, you can do things like:

```sh
$ pjs query.parachains.heads 100
{
  "heads": "0xe1efbf8cc2e1304da927986f4cd6964ce0888ce3995948bf71fe427b1a9d39b02101d2dac9c5342d7e8c4f4de2f5277ef860b3a518c1cd823b9a8cee175dce11bf7f57c5016e8a60a6cec16244b2cbf81a67a1dc7a825c288fc694997bc70e2d456400"
}
```

The collator includes its own health check, which you can inspect with

```sh
docker inspect --format='{{json .State.Health}}' cumulus_collator_1
```

The check runs every 5 minutes, and takes about a minute to complete each time. Most of that time is spent sleeping; it remains a very lightweight process.
