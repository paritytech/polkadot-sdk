# Node

ℹ️ A node -  in Polkadot - is a binary executable, whose primary purpose is to execute the [runtime](../runtime/README.md).

🔗 It communicates with other nodes in the network, and aims for
[consensus](https://wiki.polkadot.network/docs/learn-consensus) among them.

⚙️ It acts as a remote procedure call (RPC) server, allowing interaction with the blockchain.

👉 Learn more about the architecture, and the difference between a node and a runtime
[here](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/wasm_meta_protocol/index.html).

👇 Here are the most important files in this node template:

- [`chain_spec.rs`](./src/chain_spec.rs): A chain specification is a source code file that defines the chain's
initial (genesis) state.
- [`service.rs`](./src/service.rs): This file defines the node implementation.
It's a place to configure consensus-related topics.
