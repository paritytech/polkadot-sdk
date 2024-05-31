//! # Custom RPC do's and don'ts
//!
//! > Authors: @xlc, @kianenigma
//!
//! **TLDR:** don't create new custom RPCs. Instead, rely on custom Runtime APIs, combined with
//! `state_call`
//!
//! ## Background
//!
//! Polkadot-SDK offers the ability to query and subscribe storages directly. However what it did
//! not have is [view functions](https://github.com/paritytech/polkadot-sdk/issues/216). This is an
//! essential feature to avoid duplicated logic between runtime and the client SDK. Custom RPC was
//! used as a solution. It allow the RPC node to expose new RPCs that clients can be used to query
//! computed properties.
//!
//! ## Problems with Custom RPC
//!
//! Unfortunately, custom RPC comes with many problems. To list a few:
//!
//! - It is offchain logic executed by the RPC node and therefore the client has to trust the RPC
//!   node.
//! - To upgrade or add a new RPC logic, the RPC node has to be upgraded. This can cause significant
//!   trouble when the RPC infrastructure is decentralized as we will need to coordinate multiple
//!   parties to upgrade the RPC nodes.
//! - A lot of boilerplate code are required to add custom RPC.
//! - It prevents the dApp to use a light client or alternative client.
//! - It makes ecosystem tooling integration much more complicated. For example, the dApp will not
//!   be able to use [Chopsticks](https://github.com/AcalaNetwork/chopsticks) for testing as
//!   Chopsticks will not have the custom RPC implementation.
//! - Poorly implemented custom RPC can be a DoS vector.
//!
//! Hence, we should avoid custom RPC.
//!
//! ## Alternatives
//!
//! Generally, [`sc_rpc::state::StateBackend::call`] aka. `state_call` should be used instead of
//! custom RPC.
//!
//! Usually, each custom RPC comes with a corresponding runtime API which implements the business
//! logic. So instead of invoke the custom RPC, we can use `state_call` to invoke the runtime API
//! directly. This is a trivial change on the dApp and no change on the runtime side. We may remove
//! the custom RPC from the node side if wanted.
//!
//! There are some other cases that a simple runtime API is not enough. For example, implementation
//! of Ethereum RPC requires an additional offchain database to index transactions. In this
//! particular case, we can have the RPC implemented on another client.
//!
//! For example, the Acala EVM+ RPC are implemented by
//! [eth-rpc-adapter](https://github.com/AcalaNetwork/bodhi.js/tree/master/packages/eth-rpc-adapter).
//! Alternatively, the [Frontier](https://github.com/polkadot-evm/frontier) project  also provided
//! Ethereum RPC compatibility directly in the node-side software.
//!
//! ## Create a new Runtime API
//!
//! For example, let's take a look a the process through which the account nonce can be queried
//! through an RPC. First, a new runtime-api needs to be declared:
//!
#![doc = docify::embed("../../substrate/frame/system/rpc/runtime-api/src/lib.rs", AccountNonceApi)]
//!
//! This API is implemented at the runtime level, always inside [`sp_api::impl_runtime_api!`].
//!
//! As noted, this is already enough to make this API usable via `state_call`.
//!
//! ## Create a new custom RPC
//!
//! Should you wish to implement the legacy approach of exposing this runtime-api as a custom
//! RPC-api, then a custom RPC server has to be defined.
//!
#![doc = docify::embed("../../substrate/utils/frame/rpc/system/src/lib.rs", SystemApi)]
//!
//! ## Add a new RPC to the node
//!
//! Finally, this custom RPC needs to be integrated into the node side. This is usually done in a
//! `rpc.rs` in a typical template, as follows:
//!
#![doc = docify::embed("../../templates/minimal/node/src/rpc.rs", create_full)]
//!
//! ## Future
//!
//! [XCQ](https://forum.polkadot.network/t/cross-consensus-query-language-xcq/7583) will be a good
//! solution for most of the query needs.
