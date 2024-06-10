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
//!
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
//!
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
//! // TODO Radha: example of how to better link to real items. See more in https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/DOCUMENTATION_GUIDELINES.md#how-to-document
//! 6. Update [`sp_consensus_aura::AuraApi::slot_duration`] in [`sp_api::impl_runtime_apis`] to match the constant `SLOT_DURATION`
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_slot_duration)]
//!
//! 7. Implement the `AuraUnincludedSegmentApi`, which allows the collator client to query its
//!    runtime to determine whether it should author a block.
//!
//!    - Add the dependency `cumulus-primitives-aura` to the `runtime/Cargo.toml` file for your
//!      runtime
//! ```rust
//! cumulus-pallet-aura-ext = { path = "../../../../pallets/aura-ext", default-features = false }
//! cumulus-pallet-parachain-system = { path = "../../../../pallets/parachain-system",
//! default-features = false, features = ["parameterized-consensus-hook"] }
//! cumulus-pallet-session-benchmarking = { path = "../../../../pallets/session-benchmarking",
//! default-features = false } cumulus-pallet-xcm = { path = "../../../../pallets/xcm",
//! default-features = false } cumulus-pallet-xcmp-queue = { path =
//! "../../../../pallets/xcmp-queue", default-features = false, features = ["bridging"] } // here
//! cumulus-primitives-aura = { path = "../../../../primitives/aura", default-features = false }
//! ```
//!
//! - In the same file, add `"cumulus-primitives-aura/std",` to the `std` feature.
//!
//! - Inside the `impl_runtime_apis!` block for your runtime, implement the
//!   `AuraUnincludedSegmentApi` as shown below.
//! ```rust
//! impl cumulus_primitives_aura::AuraUnincludedSegmentApi<Block> for Runtime {
//!     fn can_build_upon(
//!         included_hash: <Block as BlockT>::Hash,
//!         slot: cumulus_primitives_aura::Slot,
//!     ) -> bool {
//!         ConsensusHook::can_build_upon(included_hash, slot)
//!     }
//! }
//! ```
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
//! ```rust
//! cumulus_pallet_parachain_system::register_validate_block! {
//!     Runtime = Runtime,
//!     BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
//! }
//! ```
//!
//! ## Phase 2 - Update Parachain Nodes
//!
//! This phase consists of plugging in the new lookahead collator node.
//!
//! 1. Import `cumulus_primitives_core::ValidationCode` to `node/src/service.rs`.
//! ```rust
//! use cumulus_primitives_core::{
//!     // here
//!     relay_chain::{CollatorPair, ValidationCode},
//!     ParaId,
//! };
//! ```
//!
//! 2. In `node/src/service.rs`, modify `sc_service::spawn_tasks` to use a clone of `Backend` rather
//!    than the original
//! ```rust
//! sc_service::spawn_tasks(sc_service::SpawnTasksParams {
//!     rpc_builder,
//!     client: client.clone(),
//!     transaction_pool: transaction_pool.clone(),
//!     task_manager: &mut task_manager,
//!     config: parachain_config,
//!     keystore: params.keystore_container.keystore(),
//!     backend: backend.clone(),
//!     network: network.clone(),
//!     sync_service: sync_service.clone(),
//!     system_rpc_tx,
//!     tx_handler_controller,
//!     telemetry: telemetry.as_mut(),
//! })?;
//! ```
//!
//! 3. Add `backend` as a parameter to `start_consensus()` in `node/src/service.rs`
//! ```text
//! fn start_consensus(
//!     client: Arc<ParachainClient>,
//!
//!     backend: Arc<ParachainBackend>,
//!     block_import: ParachainBlockImport,
//!     prometheus_registry: Option<&Registry>,
//!     telemetry: Option<TelemetryHandle>,
//!     task_manager: &TaskManager,
//! ```
//! ```text
//! if validator {
//!     start_consensus(
//!     client.clone(),
//!
//!     backend.clone(),
//!     block_import,
//!     prometheus_registry.as_ref(),
//! ```
//!
//! 4. In `node/src/service.rs` import the lookahead collator rather than the basic collator
//! ```rust
//!   use cumulus_client_consensus_aura::collators::lookahead::{self as aura, Params as AuraParams};
//! ```
//!
//! 5. In `start_consensus()` replace the `BasicAuraParams` struct with `AuraParams`
//!    - Change the struct type from `BasicAuraParams` to `AuraParams`
//!    - In the `para_client` field, pass in a cloned para client rather than the original
//!    - Add a `para_backend` parameter after `para_client`, passing in our para backend
//!    - Provide a `code_hash_provider` closure like that shown below
//!    - Increase `authoring_duration` from 500 milliseconds to 1500
//! ```rust
//! let params = AuraParams {
//!     create_inherent_data_providers: move |_, ()| async move { Ok(()) },
//!     block_import,
//!     para_client: client.clone(),
//!     para_backend: backend.clone(),
//!     relay_client: relay_chain_interface,
//!     code_hash_provider: move |block_hash| {
//!         client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
//!     },
//!     sync_oracle,
//!     keystore,
//!     collator_key,
//!     para_id,
//!     overseer_handle,
//!     relay_chain_slot_duration,
//!     proposer,
//!     collator_service,
//!
//!     authoring_duration: Duration::from_millis(1500),
//!     reinitialize: false,
//! };
//! ```
//!
//! **Note:** Set `authoring_duration` to whatever you want, taking your own hardware into account.
//! But if the backer who should be slower than you due to reading from disk, times out at two
//! seconds your candidates will be rejected.
//!
//! 6. In `start_consensus()` replace `basic_aura::run` with `aura::run`
//! ```rust
//! let fut = aura::run::<
//!     Block,
//!     sp_consensus_aura::sr25519::AuthorityPair,
//!     _,
//!     _,
//!     _,
//!     _,
//!     _,
//!     _,
//!     _,
//!     _,
//!     _,
//!     >(params);
//! task_manager.spawn_essential_handle().spawn("aura", None, fut);
//! ```
//!
//! ## Phase 3 - Activate Async Backing
//!
//! This phase consists of changes to your parachain’s runtime that activate async backing feature.
//!
//! 1. Configure `pallet_aura`, setting `AllowMultipleBlocksPerSlot` to true in
//!    `runtime/src/lib.rs`.
//! ```rust
//!     impl pallet_aura::Config for Runtime {
//!         type AuthorityId = AuraId;
//!         type DisabledValidators = ();
//!         type MaxAuthorities = ConstU32<100_000>;
//!         // here
//!         type AllowMultipleBlocksPerSlot = ConstBool<true>;
//!         #[cfg(feature = "experimental")]
//!         type SlotDuration = ConstU64<SLOT_DURATION>;
//!     }
//! ```
//!
//! 2. Increase the maximum `UNINCLUDED_SEGMENT_CAPACITY` in `runtime/src/lib.rs`.
//! ```rust
//! // Maximum number of blocks simultaneously accepted by the Runtime, not yet included into the
//! // relay chain.
//! pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
//! // How many parachain blocks are processed by the relay chain per parent. Limits the number of
//! // blocks authored per slot.
//! pub const BLOCK_PROCESSING_VELOCITY: u32 = 1;
//! ```
//!
//! 3. Decrease `MILLISECS_PER_BLOCK` to 6000.
//!
//! - Note: For a parachain which measures time in terms of its own block number rather than by
//!   relay block number it may be preferable to increase velocity. Changing block time may cause
//!   complications, requiring additional changes. See the section “Timing by Block Number”.
//! ```rust
//! // This determines the average expected block time that we are targeting.
//! // Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
//! // `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
//! // up by `pallet_aura` to implement `fn slot_duration()`.
//! //
//! // Change this to adjust the block time.
//! pub const MILLISECS_PER_BLOCK: u64 = 6000;
//! ```
//!
//! 4. Update `MAXIMUM_BLOCK_WEIGHT` to reflect the increased time available for block production.
//! ```rust
//! // We allow for 2 seconds of compute with a 6 second average block.
//! pub const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
//!     WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
//!     cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
//! );
//! ```
//!
//! 5. Add a feature flagged alternative for `MinimumPeriod` in `pallet_timestamp`. The type should
//!    be `ConstU64<0>` with the feature flag experimental, and `ConstU64<{SLOT_DURATION / 2}>`
//!    without.
//! ```rust
//! impl pallet_timestamp::Config for Runtime {
//!     type Moment = u64;
//!     type OnTimestampSet = Aura;
//!     #[cfg(feature = "experimental")]
//!     type MinimumPeriod = ConstU64<0>;
//!     #[cfg(not(feature = "experimental"))]
//!     type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
//!     type WeightInfo = weights::pallet_timestamp::WeightInfo<Runtime>;
//! }
//! ```
//!
//! ## Timing by Block Number
//!
//! With asynchronous backing it will be possible for parachains to opt for a block time of 6
//! seconds rather than 12 seconds. But modifying block duration isn’t so simple for a parachain
//! which was measuring time in terms of its own block number. It could result in expected and
//! actual time not matching up, stalling the parachain.
//!
//! One strategy to deal with this issue is to instead rely on relay chain block numbers for timing.
//! Relay block number is kept track of by each parachain in `pallet-parachain-system` with the
//! storage value `LastRelayChainBlockNumber`. This value can be obtained and used wherever timing
//! based on block number is needed.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
