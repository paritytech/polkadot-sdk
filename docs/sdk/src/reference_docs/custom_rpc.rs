//! # Substrate Custom RPC do's and don'ts
//! 
//! **TLDR:** don't
//! 
//! ## Background
//! 
//! Substrate offers the ability to query and subscribe storages directly. However what Substrate did not have is [view functions](https://github.com/paritytech/polkadot-sdk/issues/216). This is an essential feature to avoid duplicated logic between runtime and the client SDK. Custom RPC was used as a solution. It allow the RPC node to expose new RPCs that clients can be used to query computed properties.
//! 
//! ## Problems with Custom RPC
//! 
//! Unfortunately, custom RPC comes with many problems. To list a few:
//! 
//! - It is offchain logic executed by the RPC node and therefore the client has to trust the RPC node.
//! - To upgrade or add a new RPC logic, the RPC node has to be upgraded. This can cause significant trouble when the RPC infrastructure is decentralized as we will need to coordinate multiple parties to upgrade the RPC nodes.
//! - A lot of boilerplate code are required to add custom RPC.
//! - It prevents the dApp to use a light client or alternative client.
//! - It makes ecosystem tooling integration much more complicated. For example, the dApp will not be able to use Chopsticks for testing as Chopsticks will not have the custom RPC implemenation.
//! - Poorly implemented custom RPC can be a DoS vector.
//! 
//! Hence, we should avoid custom RPC
//! 
//! ## Alternatives
//! 
//! Generally, `state_call` should be used instead of custom RPC.
//! 
//! Usually, each custom RPC comes with a coresponding runtime API which implements the business logic. So instead of invoke the custom RPC, we can use `state_call` to invoke the runtime API directly. This is a trivial change on the dApp and no change on the runtime side. We may remove the custom RPC from the node side if wanted.
//! 
//! There are some other cases that a simple runtime API is not enough. For example, implementation of Ethereum RPC requires an addiontal offchain database to index transactions. In this particular case, we can have the RPC implemented on another client.
//! 
//! For example, the Acala EVM+ RPC are implemented by [eth-rpc-adapter](https://github.com/AcalaNetwork/bodhi.js/tree/master/packages/eth-rpc-adapter). This have a few advantages:
//! 
//! - It is easy for testing. We can launch an instance of eth-rpc-adapter have it connected to a Chopsticks instance and be able to test EVM stack using Chopsticks to do things like revert block, replay transaction, etc.
//! - No additional overhead for non RPC node as they don't need to storage the extra data that only used by the custom RPC logic.
//! - Decouple the runtime, node, and custom RPC logic. This means we can upgrade each of them indenpendly providing the runtime API remains compatible.
//! - Fully compatible with omni-node or other ecosystem tools such as Chopsticks and smoldot
//! 
//! ## Future
//! 
//! [XCQ](https://forum.polkadot.network/t/cross-consensus-query-language-xcq/7583) will be a good solution for most of the query needs.
//! 
//! ## Create a new Runtime API
//! 
//! TODO: reference to the runtime API docs
//! 
//! ## Create a new custom RPC
//! 
//! TODO: how to create a new RPC implementation
//! 
//! ## Add a new RPC to the node
//! 
//! TODO: how to add a new RPC to the node
//! 