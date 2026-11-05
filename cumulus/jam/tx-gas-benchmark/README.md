# JAM transaction gas benchmark

Measures the refine gas parachain transactions cost on JAM, offline and fully in memory: no JAM
node, no collator, no network.

It builds two blocks of `parachain-template-runtime` on the same parent the way the JAM collator
builds them, one without transactions and one with `--transactions` signed
`Balances::transfer_keep_alive`s, and runs `jam_validate_block` on each PoV the way the Parachain
Service's refine runs it. The difference, divided by the number of transactions, is the gas one
transaction costs.

## What runs where

- **Block building** (`src/block.rs`): the runtime blob runs through the node's executor on an
  in-memory client. Each block gets what the omni-node's JAM builder gives it: the refine context,
  the JAM state reader and additional-data finalizer over a JAM state proof, the Aura slot,
  `JamParent` and `BlockBundleInfo` digests, the timestamp and mocked parachain inherents, and the
  JAM block authorization seal produced with the real authorizer blob.
- **JAM state** (`src/jam_state.rs`): the auth pools, the authorizer code and the para's `ParaInfo`
  in a Gray Paper state trie. Its root is the refine context's state root and every block carries
  the proof, as against a real JAM node.
- **Refine** (`src/refine.rs`): the runtime blob runs as the Parachain Service's inner PVM, with
  the service's memory layout and child host calls, on polkavm 0.36 with the refine node's cost
  model (`Full(L2Hit)`, synchronous gas metering). A refine only counts if the runtime declares the
  built block as its head, so every reported number is from a block the runtime validated,
  including its signatures and its JAM block authorization.

The reported gas is the inner PVM's, the `invoke` part of a refine. The Parachain Service's own
work around it (loading and parsing the code, host call dispatch) is not included.

Everything is deterministic: slots, timestamps, anchors and signature nonces are fixed, so two runs
print the same numbers.

## Running it

The polkadot-sdk, parachain-service and polkajam2 checkouts have to sit next to each other.

1. Build the runtime blob for PolkaVM with the JAM configuration:

   ```sh
   SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS=" --cfg jam" \
   	cargo build --release -p parachain-template-runtime
   ```

   The blob is `target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm`.

2. Build the block authorizer in the parachain-service checkout:

   ```sh
   just build-authorizer-pvm
   ```

   The blob is
   `target/pvm/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam`.

3. Run the benchmark:

   ```sh
   SKIP_WASM_BUILD=1 cargo run --release -p cumulus-jam-tx-gas-benchmark -- \
   	--runtime target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm \
   	--authorizer ../parachain-service/target/pvm/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam \
   	--transactions 50
   ```

   `SKIP_WASM_BUILD` keeps the build from compiling the runtime to WASM: the benchmark only uses
   the runtime's types natively and runs the blob passed with `--runtime`.
