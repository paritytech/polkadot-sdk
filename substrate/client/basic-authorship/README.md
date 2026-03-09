Basic implementation of block-authoring logic.

## MEV Shield integration

When a `ShieldKeystorePtr` is provided to `ProposerFactory`, the proposer handles shielded transactions during block authoring:

1. Each pending transaction is inspected via the `ShieldApi` runtime API (`try_decode_shielded_tx`) to detect whether it is a shielded wrapper.
2. The proposer calls `is_shielded_using_current_key` (at the parent hash) with the transaction's `key_hash` to check whether the transaction is encrypted with the key this proposer can decrypt. Transactions encrypted with a different author's key are silently skipped — they stay in the pool for the correct author.
3. Matching transactions are decrypted using the keystore's `current_dec_key`. Block size accounting is adjusted to reflect the inner plaintext size rather than the ciphertext size.
4. If the wrapper extrinsic applies successfully, the decrypted inner transaction is immediately pushed into the block as a second extrinsic.

Decryption happens only at block authoring time, never earlier. Setting the environment variable `SUBSTRATE_SKIP_SHIELDED_TXS=1` causes the proposer to skip all shielded transactions (leaving them in the pool for other authors).

See [`stp-shield`](../../primitives/shield) and [`stc-shield`](../shield) for the underlying types and keystore implementation.

# Example

```rust
// The first step is to create a `ProposerFactory`.
let mut proposer_factory = ProposerFactory::new(client.clone(), txpool.clone(), None);

// From this factory, we create a `Proposer`.
let proposer = proposer_factory.init(
	&client.header(client.chain_info().genesis_hash).unwrap().unwrap(),
);

// The proposer is created asynchronously.
let proposer = futures::executor::block_on(proposer).unwrap();

// This `Proposer` allows us to create a block proposition.
// The proposer will grab transactions from the transaction pool, and put them into the block.
let future = proposer.propose(
	Default::default(),
	Default::default(),
	Duration::from_secs(2),
);

// We wait until the proposition is performed.
let block = futures::executor::block_on(future).unwrap();
println!("Generated block: {:?}", block.block);
```


License: GPL-3.0-or-later WITH Classpath-exception-2.0
