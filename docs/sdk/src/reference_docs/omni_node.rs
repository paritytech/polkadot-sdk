//! # (Omni) Node
//!
//! This reference doc elaborates on what a Polkadot-SDK/Substrate node software is, and what
//! various ways exist to run one.
//!
//! The node software, as denoted in [`crate::reference_docs::wasm_meta_protocol`], is everything in
//! a blockchain other than the WASM runtime. It contains common components such as the database,
//! networking, RPC server and consensus. Substrate-based nodes are native binaries that are
//! compiled down from the Rust source code. The `node` folder in any of the [`templates`] are
//! examples of this source.
//!
//! > Note: A typical node also contains a lot of other tools (exposed as subcommands) that are
//! > useful for operating a blockchain, such as the ones noted in
//! > [`polkadot_omni_node_lib::cli::Cli::subcommand`].
//!
//! ## Node <> Runtime Interdependence
//!
//! While in principle the node can be mostly independent of the runtime, for various reasons, such
//! as the [native runtime](crate::reference_docs::wasm_meta_protocol#native-runtime), the node and
//! runtime were historically tightly linked together. Another reason is that the node and the
//! runtime need to be in agreement about which consensus algorithm they use, as described
//! [below](#consensus-engine).
//!
//! Specifically, the node relied on the existence of a linked runtime, and *could only reliably run
//! that runtime*. This is why if you look at any of the [`templates`], they are all composed of a
//! node, and a runtime.
//!
//! Moreover, the code and API of each of these nodes was historically very advanced, and tailored
//! towards those who wish to customize many of the node components at depth.
//!
//! > The notorious `service.rs` in any node template is a good example of this.
//!
//! A [trend](https://github.com/paritytech/polkadot-sdk/issues/62) has already been undergoing in
//! order to de-couple the node and the runtime for a long time. The north star of this effort is
//! twofold :
//!
//! 1. develop what can be described as an "omni-node": A node that can run most runtimes.
//! 2. provide a cleaner abstraction for creating a custom node.
//!
//! While a single omni-node running *all possible runtimes* is not feasible, the
//! [`polkadot-omni-node`] is an attempt at creating the former, and the [`polkadot_omni_node_lib`]
//! is the latter.
//!
//! > Note: The OmniNodes are mainly focused on the development needs of **Polkadot
//! > parachains ONLY**, not (Substrate) solo-chains. For the time being, solo-chains are not
//! > supported by the OmniNodes. This might change in the future.
//!
//! ## Types of Nodes
//!
//! With the emergence of the OmniNodes, let's look at the various Node options available to a
//! builder.
//!
//! ### [`polkadot-omni-node`]
//!
//! [`polkadot-omni-node`] is a white-labeled binary, released as a part of Polkadot SDK that is
//! capable of meeting the needs of most Polkadot parachains.
//!
//! It can act as the collator of a parachain in production, with all the related auxillary
//! functionalities that a normal collator node has: RPC server, archiving state, etc. Moreover, it
//! can also run the wasm blob of the parachain locally for testing and development.
//!
//! ### [`polkadot_omni_node_lib`]
//!
//! [`polkadot_omni_node_lib`] is the library version of the above, which can be used to create a
//! fresh parachain node, with a some limited configuration options using a lean API.
//!
//! ### Old School Nodes
//!
//! The existing node architecture, as seen in the [`templates`], is still available for those who
//! want to have full control over the node software.
//!
//! ### Summary
//!
//! We can summarize the choices for the node software of any given user of Polkadot-SDK, wishing to
//! deploy a parachain into 3 categories:
//!
//! 1. **Use the [`polkadot-omni-node`]**: This is the easiest way to get started, and is the most
//!   likely to be the best choice for most users.
//!     * can run almost any runtime with [`--dev-block-time`]
//! 2. **Use the [`polkadot_omni_node_lib`]**: This is the best choice for those who want to have
//!    slightly more control over the node software, such as embedding a custom chain-spec.
//! 3. **Use the old school nodes**: This is the best choice for those who want to have full control
//!    over the node software, such as changing the consensus engine, altering the transaction pool,
//!    and so on.
//!
//! ## _OmniTools_: User Journey
//!
//! All in all, the user journey of a team/builder, in the OmniNode world is as follows:
//!
//! * The [`templates`], most notably the [`parachain-template`] is the canonical starting point.
//!   That being said, the node code of the templates (which may be eventually
//!   removed/feature-gated) is no longer of relevance. The only focus is in the runtime, and
//!   obtaining a `.wasm` file. References:
//!     * [`crate::guides::your_first_pallet`]
//!     * [`crate::guides::your_first_runtime`]
//! * If need be, the weights of the runtime need to be updated using `frame-omni-bencher`.
//!   References:
//!     * [`crate::reference_docs::frame_benchmarking_weight`]
//! * Next, [`chain-spec-builder`] is used to generate a `chain_spec.json`, either for development,
//!   or for production. References:
//!     * [`crate::reference_docs::chain_spec_genesis`]
//! * For local development, the following options are available:
//!     * `polkadot-omni-node` (notably, with [`--dev-block-time`]). References:
//!         * [`crate::guides::your_first_node`]
//!     * External tools such as `chopsticks`, `zombienet`.
//!         * See the `README.md` file of the `polkadot-sdk-parachain-template`.
//! * For production `polkadot-omni-node` can be used out of the box.
//! * For further customization [`polkadot_omni_node_lib`] can be used.
//!
//! ## Appendix
//!
//! This section describes how the interdependence between the node and the runtime is related to
//! the consensus engine. This information is useful for those who want to understand the
//! historical context of the node and the runtime.
//!
//! ### Consensus Engine
//!
//! In any given substrate-based chain, both the node and the runtime will have their own
//! opinion/information about what consensus engine is going to be used.
//!
//! In practice, the majority of the implementation of any consensus engine is in the node side, but
//! the runtime also typically needs to expose a custom runtime-api to enable the particular
//! consensus engine to work, and that particular runtime-api is implemented by a pallet
//! corresponding to that consensus engine.
//!
//! For example, taking a snippet from [`solochain_template_runtime`], the runtime has to provide
//! this additional runtime-api (compared to [`minimal_template_runtime`]), if the node software is
//! configured to use the Aura consensus engine:
//!
//! ```text
//! impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
//!     fn slot_duration() -> sp_consensus_aura::SlotDuration {
//!         ...
//!     }
//!     fn authorities() -> Vec<AuraId> {
//!         ...
//!     }
//! }
//! ```
//!
//! For simplicity, we can break down "consensus" into two main parts:
//!
//! * Block Authoring: Deciding who gets to produce the next block.
//! * Finality: Deciding when a block is considered final.
//!
//! For block authoring, there are a number of options:
//!
//! * [`sc_consensus_manual_seal`]: Useful for testing, where any node can produce a block at any
//!   time. This is often combined with a fixed interval at which a block is produced.
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
//! ### Consequences for OmniNode
//!
//!
//! The consequence of the above is that anyone using the OmniNode must also be aware of the
//! consensus system used in the runtime, and be aware if it is matching that of the OmniNode or
//! not. For the time being, [`polkadot-omni-node`] only supports:
//!
//! * Parachain-based Aura consensus, with 6s async-backing block-time, and before full elastic
//!   scaling). [`polkadot_omni_node_lib::cli::Cli::experimental_use_slot_based`] for fixed factor
//!   scaling (a step
//! * Ability to run any runtime with [`--dev-block-time`] flag. This uses
//!   [`sc_consensus_manual_seal`] under the hood, and has no restrictions on the runtime's
//!   consensus.
//!
//! [This](https://github.com/paritytech/polkadot-sdk/issues/5565) future improvement to OmniNode
//! aims to make such checks automatic.
//!
//!
//! [`templates`]: crate::polkadot_sdk::templates
//! [`parachain-template`]: https://github.com/paritytech/polkadot-sdk-parachain-template
//! [`--dev-block-time`]: polkadot_omni_node_lib::cli::Cli::dev_block_time
//! [`polkadot-omni-node`]: https://crates.io/crates/polkadot-omni-node
//! [`chain-spec-builder`]: https://crates.io/crates/staging-chain-spec-builder
