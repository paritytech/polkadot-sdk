//! # Enable elastic scaling for a parachain
//!
//! <div class="warning">This guide assumes full familiarity with Asynchronous Backing and its
//! terminology, as defined in <a href="https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/async_backing_guide/index.html">the Polkadot SDK Docs</a>.
//! </div>
//!
//! ## Quick introduction to Elastic Scaling
//!
//! [Elastic scaling](https://www.parity.io/blog/polkadot-web3-cloud)
//! is a feature that enables parachains (rollups) to use multiple cores.
//! Parachains can adjust their usage of core resources on the fly to increase TPS and decrease latency.
//! 
//! ### When do you need Elastic Scaling?
//! 
//! Depending on their use case, applications might have an increased need for the following:
//! - compute (CPU weight)
//! - bandwidth (proof size)
//! - lower latency (block time)
//! 
//! ### High throughput (TPS) and lower latency
//! 
//! If the main bottleneck is the CPU, then your parachain needs to maximize the compute usage of each core while also achieving 
//! a lower latency.
//! 3 cores provide the best balance between CPU, bandwidth and latency: up to 6s of execution, 3MB/s of DA bandwidth and fast 
//! block time of just 2 seconds.
//! 
//! ### High bandwidth 
//! 
//! Useful for applications that are bottlenecked by bandwidth.
//! By using 6 cores, applications can make use of up to 6s of compute, 6MB/s of bandwidth every 6s while also achieving 
//! 1 second block times. 
//! 
//! ### Ultra low latency
//! When latency is the primary requirement, Elastic scaling is currently the only solution. The caveat is
//! the efficiency of core time usage decreases as more cores are used. 
//! 
//! For example, using 12 cores enables fast transaction confirmations with 500ms blocks and up to 12 MB/s of DA bandwidth.
//!
//! ## Dependencies
//! 
//! Prerequisites: Polkadot-SDK `2509` or newer.
//! 
//! To ensure the security and reliability of your chain when using this feature you need the following:
//! - An omni-node based collator. This has already become the default choice for collators.
//! - RFC 103. This is mandatory protection against PoV replay attacks.
//! - Enabling the relay parent offset feature. This is required to ensure the parachain block times 
//!   and transaction in-block confidence is not negatively affected by relay chain forks.
//! - Block production configuration adjustments.
//! 
//! ### Upgrade to Polkadot Omni node
//! 
//! Your collators need to run `polkadot-parachain` or `polkadot-omni-node` with the `--authoring slot-based` CLI argument.
//! 
//! Further information about omni-node and how to upgrade is available:
//! - [high level docs](https://docs.polkadot.com/develop/toolkit/parachains/polkadot-omni-node/)
//! - [`crate::reference_docs::omni_node`]
//! 
//! ### Enable RFC103
//!
//! RFC103 is enabled automatically on the collators if it is enabled on the relay chain. There are no code changes
//! required on the client to support it. 
//! 
//! RFC103 relies on the ability of parachain blocks to commit to a specific core index on the relay chain.
//! This commitment is implemented via `UMP` signals, which rely on the upward message passing channel that
//! is used by parachains to send messages to the relay chain.
//! 
//! The only required change for the runtime is enabling the `experimental-ump-signals` feature of the `parachain-system`
//! pallet:
//! `cumulus-pallet-parachain-system = { workspace = true, features = ["experimental-ump-signals"] }`
//! 
//! ### Enable the relay parent offset feature
//! 
//! It is recommended to use an offset of `1`, which is sufficient to eliminate any issues 
//! with relay chain forks.
//! 
//! Configure the relay parent offset like this:
//! ```ignore
//!     /// Build with an offset of 1 behind the relay chain best block.
//!     const RELAY_PARENT_OFFSET: u32 = 1;
//! 
//!     impl cumulus_pallet_parachain_system::Config for Runtime {
//!         // ... 
//!         type RelayParentOffset = ConstU32<RELAY_PARENT_OFFSET>;
//!     }
//! ```
//!
//! Implement the runtime API to retrieve the offset on the client side.
//! ```ignore
//!     impl cumulus_primitives_core::RelayParentOffsetApi<Block> for Runtime {
//!         fn relay_parent_offset() -> u32 {
//!             RELAY_PARENT_OFFSET
//!         }
//!     }
//! ```
//! 
//! ### Block production configuration
//!
//! This configuration directly controls the minimum block time and maximum number of cores
//! the parachain can use.
//!
//! Example configuration for a 3 core parachain:
//!  ```rust 
//!     /// How many parachain blocks are processed by the relay chain per parent. Limits the
//!     /// number of blocks authored per slot. This determines the minimum block time of the parachain: 
//!     // `RELAY_CHAIN_SLOT_DURATION_MILLIS/BLOCK_PROCESSING_VELOCITY`
//!     const BLOCK_PROCESSING_VELOCITY: u32 = 3;
//!
//!     /// Maximum number of blocks simultaneously accepted by the Runtime, not yet included
//!     /// into the relay chain.
//!     const UNINCLUDED_SEGMENT_CAPACITY: u32 = 2 * BLOCK_PROCESSING_VELOCITY + 1;
//! 
//!     /// Relay chain slot duration, in milliseconds.
//!     const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;
//! 
//!     type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
//!         Runtime,
//!         RELAY_CHAIN_SLOT_DURATION_MILLIS,
//!         BLOCK_PROCESSING_VELOCITY,
//!         UNINCLUDED_SEGMENT_CAPACITY,
//!     >;
//!
//!  ```
//! 
//! ## Current limitations
//!
//! ### Maximum execution time per relay chain block.

//!    Since parachain block authoring is sequential, the next block can only be built after 
//!    the previous one has been imported.
//!    At present, a core allows up to 2 seconds of execution per relay chain block.
//!
//!    If we assume a 6s parachain slot, and each block takes the full 2 seconds to execute, 
//!    the parachain will not be able to fully utilize the compute resources of all 3 cores.
//!    
//!    If the collator hardware is faster, it can author and import full blocks more quickly,
//!    making it possible to utilize even more than 3 cores efficiently.
//! 
//! #### Why ?
//! 
//!    Within a 6-second parachain slot, collators can author multiple parachain blocks.
//!    Before building the first block in a slot, the new block author must import the last 
//!    block produced by the previous author.
//!    If the import of the last block is not completed before the next relay chain slot starts,
//!    the new author will build on its parent (assuming it was imported).
//!    This means that, on reference hardware, a parachain with a slot time of 6 seconds can effectively 
//!    utilize up to 4 seconds of execution per relay chain block, because it needs to ensure the 
//!    next block author has enough time to import the last block.
//!    Hardware with higher single-core performance can enable a parachain to fully utilize more cores.
//!    
//! ### Fixed factor scaling. 
//!    For true elasticity, a parachain needs to acquire more cores when needed in an automated manner. 
//!    This functionality is not yet available in the SDK. So, acquiring additional on-demand or bulk cores
//!    has to be managed externally.
//!
