//! # Upgrade Parachain for Asynchronous Backing Compatibility
//!
//! This guide is relevant for cumulus based parachain projects started in 2023 or before, whose
//! backing process is synchronous where parablocks can only be built on the latest Relay Chain
//! block. Async Backing allows collators to build parablocks on older Relay Chain blocks and create
//! pipelines of multiple pending parablocks. This parallel block generation increases efficiency
//! and throughput. For more information on Async backing and its terminology, refer to the document
//! on [the Polkadot Wiki.](https://wiki.polkadot.network/docs/maintain-guides-async-backing)
//!
//! > If starting a new parachain project, please use an async backing compatible template such as
//! > the
//! > [parachain template](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).
//! The rollout process for Async Backing has three phases. Phases 1 and 2 below put new
//! infrastructure in place. Then we can simply turn on async backing in phase 3.
//!
//! ## Prerequisite
//!
//! The relay chain needs to have async backing enabled so double-check that the relay-chain
//! configuration contains the following three parameters (especially when testing locally e.g. with
//! zombienet):
//!
//! ```json
//! "async_backing_params": {
//!     "max_candidate_depth": 3,
//!     "allowed_ancestry_len": 2
//! },
//! "scheduling_lookahead": 2
//! ```
//!
//! <div class="warning">`scheduling_lookahead` must be set to 2, otherwise parachain block times
//! will degrade to worse than with sync backing!</div>
//!
//! ## Phase 1 - Update Parachain Runtime
//!
//! This phase involves configuring your parachain’s runtime `/runtime/src/lib.rs` to make use of
//! async backing system.
//!
//! 1. Establish and ensure constants for `capacity` and `velocity` are both set to 1 in the
//!    runtime.
//! 2. Establish and ensure the constant relay chain slot duration measured in milliseconds equal to
//!    `6000` in the runtime.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", async_backing_params)]
//!
//! 3. Establish constants `MILLISECS_PER_BLOCK` and `SLOT_DURATION` if not already present in the
//!    runtime.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", block_times)]
//!
//! 4. Configure `cumulus_pallet_parachain_system` in the runtime.
//!
//! - Define a `FixedVelocityConsensusHook` using our capacity, velocity, and relay slot duration
//! constants. Use this to set the parachain system `ConsensusHook` property.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", ConsensusHook)]
//! ```rust
//! impl cumulus_pallet_parachain_system::Config for Runtime {
//!     ..
//!     type ConsensusHook = ConsensusHook;
//!     ..
//! }
//! ```
//! - Set the parachain system property `CheckAssociatedRelayNumber` to
//! `RelayNumberMonotonicallyIncreases`
//! ```ignore
//! impl cumulus_pallet_parachain_system::Config for Runtime {
//! 	..
//! 	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
//! 	..
//! }
//! ```
//! 
//! 5. Configure `pallet_aura` in the runtime.
//!
//! - Set `AllowMultipleBlocksPerSlot` to `false` (don't worry, we will set it to `true` when we
//! activate async backing in phase 3).
//!
//! - Define `pallet_aura::SlotDuration` using our constant `SLOT_DURATION`
//! ```ignore
//! impl pallet_aura::Config for Runtime {
//! 	..
//! 	type AllowMultipleBlocksPerSlot = ConstBool<false>;
//! 	#[cfg(feature = "experimental")]
//! 	type SlotDuration = ConstU64<SLOT_DURATION>;
//! 	..
//! }
//! ```
//!
//! 6. Update `sp_consensus_aura::AuraApi::slot_duration` in `sp_api::impl_runtime_apis` to match the constant `SLOT_DURATION`
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_slot_duration)]
//! 
//! 7. Implement the `AuraUnincludedSegmentApi`, which allows the collator client to query its
//!    runtime to determine whether it should author a block.
//!
//!    - Add the dependency `cumulus-primitives-aura` to the `runtime/Cargo.toml` file for your
//!      runtime
//! ```rust
//! ..
//! cumulus-primitives-aura = { path = "../../../../primitives/aura", default-features = false }
//! ..
//! ```
//!
//! - In the same file, add `"cumulus-primitives-aura/std",` to the `std` feature.
//!
//! - Inside the `impl_runtime_apis!` block for your runtime, implement the
//!   `cumulus_primitives_aura::AuraUnincludedSegmentApi` as shown below.
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_can_build_upon)]
//!
//! **Note:** With a capacity of 1 we have an effective velocity of ½ even when velocity is
//! configured to some larger value. This is because capacity will be filled after a single block is
//! produced and will only be freed up after that block is included on the relay chain, which takes
//! 2 relay blocks to accomplish. Thus with capacity 1 and velocity 1 we get the customary 12 second
//! parachain block time.
//!
//! 8. If your `runtime/src/lib.rs` provides a `CheckInherents` type to `register_validate_block`,
//!    remove it. `FixedVelocityConsensusHook` makes it unnecessary. The following example shows how
//!    `register_validate_block` should look after removing `CheckInherents`.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", register_validate_block)]
//!
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
