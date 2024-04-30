//! # Node and Chain Specification
//!
//! This reference docs elaborates on different ways to run a node software, and an important
//! component of any node, the chain specification. To learn more about the node, see
//! [`crate::reference_docs::wasm_meta_protocol`].
//!
//! <div class="warning">
//!
//! Note that the material here is tentative as a lot of the processes related to node and
//! chain-spec are under active development. See:
//! * <https://github.com/paritytech/polkadot-sdk/pull/3597/>
//! * <https://github.com/paritytech/polkadot-sdk/issues/62>
//! * <https://github.com/paritytech/polkadot-sdk/issues/5>
//!
//! </div>
//!
//! Let's take a step back, and recap some of the software components that make up the node. Most
//! importantly, the node is composed of:
//!
//! * Consensus Engine
//! * Chain Specification
//! * RPC server, Database, P2P networking, Transaction Pool etc.
//!
//! Our main focus will be on the former two.
//!
//! ## Consensus Engine
//!
//! In any given substrate-based chain, both the node and the runtime will have their own
//! opinion/information about what consensus engine is going to be used.
//!
//! In practice, the majority of the implementation of any consensus engine is in the node side, but
//! the runtime also typically needs to expose a custom runtime-api to enable the particular
//! consensus engine to work, and that particular runtime-api is implemented by a pallet
//! corresponding to that consensus engine.
//!
//! For example, taking a snippet from [`solochain-template-runtime`], the runtime has to provide
//! this additional runtime-api, if the node software is configured to use Aura:
//!
//! ```ignore
//! impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
//!     fn slot_duration() -> sp_consensus_aura::SlotDuration {
//!         ...
//!     }
//!     fn authorities() -> Vec<AuraId> {
//!         ...
//!     }
//! }
//! ````
//!
//! For simplicity, we can break down "consensus" into two main parts:
//!
//! * Block Authoring: Deciding who gets to produce the next block.
//! * Finality: Deciding when a block is considered final.
//!
//! For block authoring, there are a number of options:
//!
//! * [`sc_consensus_manual_seal`]: Useful for testing, where any node can produce a block.
//! * [`sc_consensus_aura`]/[`pallet_aura`]: A simple round-robin block authoring mechanism.
//! * [`sc_consensus_babe`]/[`pallet_babe`]: A more advanced block authoring mechanism, capable of
//!   anonymizing the next block author.
//! * [`sc_consensus_pow`]: Proof of Work block authoring.
//!
//! For finality, there is one main option shipped with polkadot-sdk:
//!
//! * [`sc_consensus_grandpa`]/[`pallet_grandpa`]: A finality gadget that uses a voting mechanism to
//!   decide when a block
//!
//! **The most important lesson here is that the node and the runtime must have matching consensus
//! components.**
//!
//! ## Chain Specification
//!
//! TODO: brief intro into chain, spec, why it matters, how the node can be linked to it. How part
//! of the chain spec is the genesis config. also forward to [`sc_chain_spec`].
//!
//! ## Node Types
//!
//! This then brings us to explore what options are available to you in terms of node software when
//! using polkadot-sdk. Historically, the one and only way has been to use
//! [templates](crate::polkadot_sdk::templates), but we expect more options to be released in 2024.
//!
//! ### Using a Full Node via Templates
//!
//! In this option, your project will contain the full runtime+node software, and the two components
//! are aware of each other's details. For example, in any given template, both the node and the
//! runtime are configured to use the same, and correct consensus.
//!
//! This usually entails a lot of boilerplate code, especially on the node side, and therefore using
//! one of our many [`crate::polkadot_sdk::templates`] is the recommended way to get started with
//! this.
//!
//! The advantage of this option is that you will have full control over customization of your node
//! side components. The downside is that there is more code to maintain, especially when it comes
//! to keeping up with new releases.
//!
//! ### Using an Omni-Node
//!
//! An omni-node is a new term in the polkadot-sdk (see
//! [here](https://github.com/paritytech/polkadot-sdk/pull/3597/) and
//! [here](https://github.com/paritytech/polkadot-sdk/issues/5)) and refers to a node that is
//! capable of running any runtime, so long as a certain set of assumptions are met. One of the most
//! important of such assumptions is that the consensus engine, as explained above, must match
//! between the node and runtime.
//!
//! Therefore we expect to have "one omni-node per consensus type".
//!
//! The end goal with the omni-nodes is for developers to not need to maintain any node software and
//! download binary which can run their runtime.
//!
//! Given polkadot-sdk's path toward totally [deprecating the native
//! runtime](https://github.com/paritytech/polkadot-sdk/issues/62) from one another, using an
//! omni-node is the natural evolution. Read more in
//! [`crate::reference_docs::wasm_meta_protocol#native-runtime`].
