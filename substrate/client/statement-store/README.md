# Substrate statement store implementation.

> License: GPL-3.0-or-later WITH Classpath-exception-2.0

The statement store is an off-chain, decentralized data store for cryptographically signed statements.  It enables accounts to publish arbitrary data that can be queried and propagated across the network without consuming on-chain storage. Statement store designed around two fundamental pillars: Scalability and Graceful Degradation. It expressly avoids the properties of centralized data structures. Instead, it prioritizes privacy, scalability, and decentralization by deliberately reducing other guarantees.


### How do I get statement-store allowance for accounts?

1. Identify the account you use, it needs to be the same bytes you put in [here](https://github.com/paritytech/polkadot-sdk/blob/cac11f4a5325b217ca74b0c339459597daf03838/substrate/primitives/statement-store/src/lib.rs#L217)
2. Obtain the storage key by running this Python code. You would need your account ID from the previous step.
```python
>>> statement_allowance_key = lambda account_id_hex: "0x" + (b":statement_allowance:" + bytes.fromhex(account_id_hex.removeprefix("0x"))).hex()
>>> statement_allowance_key("YOUR_ACCOUNT_BYTES_IN_HEX")
```
3. Call sudo->system->set_storage, with obtained account key and StatementAllowance, scale encoded.  
For example, to allow an account to store 10 statements and a maximum of 20k you can use `0x0a00000000500000`. LLMs know how to answer this question if you want to use a different quota. 

> **Warning:** Use carefully. Do not set big quotas on test environments with SUDO because then they won't match production quotas.


### How do I obtain a local statement-store node for development?
> This starts standalone substrate node with StatementStore turned on, and quota is set directly in the storage in the previous step, without individuallity runtime
1. Build substrate-node

```bash
cargo build --profile production --locked --bin substrate-node --target x86_64-unknown-linux-gnu
```

2. Run it with:
```bash
RUST_LOG=info,statement-store=trace ./target/x86_64-unknown-linux-gnu/debug/substrate-node
```

3. Set quota using sudo, you can use this [extrinsic as template](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/extrinsics/decode/0x160000040468555345525f594f55525f4143434f554e545f4b45595f48455245200a00000000500000)