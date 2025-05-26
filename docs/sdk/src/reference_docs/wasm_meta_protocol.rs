//! # WASM Meta Protocol
//!
//! All Substrate based chains adhere to a unique architectural design novel to the Polkadot
//! ecosystem. We refer to this design as the "**WASM Meta Protocol**".
//!
//! Consider the fact that a traditional blockchain software is usually a monolithic artifact.
//! **Upgrading any part of the system implies upgrading the entire system**. This has historically
//! led to cumbersome forkful upgrades to be the status quo in blockchain ecosystems. In other
//! words, the entire node software is the specification of the blockchain's [`state transition
//! function`](crate::reference_docs::blockchain_state_machines).
//!
//! Moreover, the idea of "storing code in the state" is explored in the context of smart contracts
//! platforms, but has not been expanded further.
//!
//! Substrate mixes these two ideas together, and takes the novel approach of storing the
//! blockchain's main "state transition function" in the main blockchain state, in the same fashion
//! that a smart contract platform stores the code of individual contracts in its state. As noted in
//! [`crate::reference_docs::blockchain_state_machines`], this state transition function is called
//! the **Runtime**, and WASM is chosen as the bytecode. The Runtime is stored under a special key
//! in the state (see [`sp_core::storage::well_known_keys`]) and can be updated as a part of the
//! state transition function's execution, just like a user's account balance can be updated.
//!
//! > Note that while we drew an analogy between smart contracts and runtimes in the above, there
//! > are fundamental differences between the two, explained in
//! > [`crate::reference_docs::runtime_vs_smart_contract`].
//!
//! The rest of the system that is NOT the state transition function is called the
//! [**Node**](crate::reference_docs::glossary#node), and is a normal binary that is compiled from
//! Rust to different hardware targets.
//!
//! This design enables all Substrate-based chains to be fork-less-ly upgradeable, because the
//! Runtime can be updated on the fly, within the execution of a block, and the node is (for the
//! most part) oblivious to the change that is happening.
//!
//! Therefore, the high-level architecture of a any Substrate-based chain can be demonstrated as
//! follows:
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
//!
//! The node and the runtime need to communicate. This is done through two concepts:
//!
//! 1. **Host functions**: a way for the (WASM) runtime to talk to the node. All host functions are
//!    defined in [`sp_io`]. For example, [`sp_io::storage`] are the set of host functions that
//!    allow the runtime to read and write data to the on-chain state.
//! 2. **Runtime APIs**: a way for the node to talk to the WASM runtime. Runtime APIs are defined
//!    using macros and utilities in [`sp_api`]. For example, [`sp_api::Core`] is the most
//!    fundamental runtime API that any blockchain must implement in order to be able to (re)
//!    execute blocks.
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_client_runtime.mmd")]
//!
//! A runtime must have a set of runtime APIs in order to have any meaningful blockchain
//! functionality, but it can also expose more APIs. See
//! [`crate::reference_docs::custom_runtime_api_rpc`] as an example of how to add custom runtime
//! APIs to your FRAME-based runtime.
//!
//! Similarly, for a runtime to be "compatible" with a node, the node must implement the full set of
//! host functions that the runtime at any point in time requires. Given the fact that a runtime can
//! evolve in time, and a blockchain node (typically) wishes to be capable of re-executing all the
//! previous blocks, this means that a node must always maintain support for the old host functions.
//! **This implies that adding a new host function is a big commitment and should be done with
//! care**. This is why, for example, adding a new host function to Polkadot always requires an RFC.
//! Learn how to add a new host function to your runtime in
//! [`crate::reference_docs::custom_host_functions`].
//!
//! ## Node vs. Runtime
//!
//! A common question is: which components of the system end up being part of the node, and which
//! ones of the runtime?
//!
//! Recall from [`crate::reference_docs::blockchain_state_machines`] that the runtime is the state
//! transition function. Anything that needs to influence how your blockchain's state is updated,
//! should be a part of the runtime. For example, the logic around currency, governance, identity or
//! any other application-specific logic that has to do with the state is part of the runtime.
//!
//! Anything that does not have to do with the state-transition function and will only
//! facilitate/enable it is part of the node. For example, the database, networking, and even
//! consensus algorithm are all node-side components.
//!
//! > The consensus is to your runtime what HTTP is to a web-application. It is the underlying
//! > engine that enables trustless execution of the runtime in a distributed manner whilst
//! > maintaining a canonical outcome of that execution.
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_with_frame.mmd")]
//!
//! ## State
//!
//! From the previous sections, we know that the database component is part of the node, not the
//! runtime. We also hinted that a set of host functions ([`sp_io::storage`]) are how the runtime
//! issues commands to the node to read/write to the state. Let's dive deeper into this.
//!
//! The state of the blockchain, what we seek to come to consensus about, is indeed *kept* in the
//! node side. Nonetheless, the runtime is the only component that:
//!
//! 1. Can update the state.
//! 2. Can fully interpret the state.
//!
//! In fact, [`sp_core::storage::well_known_keys`] are the only state keys that the node side is
//! aware of. The rest of the state, including what logic the runtime has, what balance each user
//! has and such, are all only comprehensible to the runtime.
#![doc = simple_mermaid::mermaid!("../../../mermaid/state.mmd")]
//!
//! In the above diagram, all of the state keys and values are opaque bytes to the node. The node
//! does not know what they mean, and it does not know what is the type of the corresponding value
//! (e.g. if it is a number of a vector). Contrary, the runtime knows both the meaning of their
//! keys, and the type of the values.
//!
//! This opaque-ness is the fundamental reason why Substrate-based chains can fork-less-ly upgrade:
//! because the node side code is kept oblivious to all of the details of the state transition
//! function. Therefore, the state transition function can freely upgrade without the node needing
//! to know.
//!
//! ## Native Runtime
//!
//! Historically, the node software also kept a native copy of the runtime at the time of
//! compilation within it. This used to be called the "Native Runtime". The main purpose of the
//! native runtime used to be leveraging the faster execution time and better debugging
//! infrastructure of native code. However, neither of the two arguments strongly hold and the
//! native runtime is being fully removed from the node-sdk.
//!
//! See: <https://github.com/paritytech/polkadot-sdk/issues/62>
//!
//! > Also, note that the flags [`sc_cli::ExecutionStrategy::Native`] is already a noop and all
//! > chains built with Substrate only use WASM execution.
//!
//! ### Runtime Versions
//!
//! An important detail of the native execution worth learning about is that the node software,
//! obviously, only uses the native runtime if it is the same code as with the wasm blob stored
//! onchain. Else, nodes who run the native runtime will come to a different state transition. How
//! do nodes determine if two runtimes are the same? Through the very important
//! [`sp_version::RuntimeVersion`]. All runtimes expose their version via a runtime api
//! ([`sp_api::Core::version`]) that returns this struct. The node software, or other applications,
//! inspect this struct to examine the identity of a runtime, and to determine if two runtimes are
//! the same. Namely, [`sp_version::RuntimeVersion::spec_version`] is the main key that implies two
//! runtimes are the same.
//!
//! Therefore, it is utmost important to make sure before any runtime upgrade, the spec version is
//! updated.
//!
//! ## Example: Block Execution.
//!
//! As a final example to recap, let's look at how Substrate-based nodes execute blocks. Blocks are
//! received in the node side software as opaque blobs and in the networking layer.
//!
//! At some point, based on the consensus algorithm's rules, the node decides to import (aka.
//! *validate*) a block.
//!
//! * First, the node will fetch the state of the parent hash of the block that wishes to be
//! imported.
//! * The runtime is fetched from this state, and placed into a WASM execution environment.
//! * The [`sp_api::Core::execute_block`] runtime API is called and the block is passed in as an
//! argument.
//! * The runtime will then execute the block, and update the state accordingly. Any state update is
//!   issued via the [`sp_io::storage`] host functions.
//! * Both the runtime and node will check the state-root of the state after the block execution to
//!   match the one claimed in the block header.
//!
//! > Example taken from [this
//! > lecture](https://www.youtube.com/watch?v=v0cKuddbF_Q&list=PL-w_i5kwVqbkRmfDn5nzeuU1S_FFW8dDg&index=4)
//! > of the Polkadot Blockchain Academy.
