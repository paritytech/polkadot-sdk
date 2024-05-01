//! # Upgrade Parachain for Asynchronous Backing Compatibility
//!
//! This guide is relevant for cumulus based parachain projects started in 2023 or before. Later
//! projects should already be async backing compatible. If starting a new parachain project, please use
//! an async backing compatible template such as
//! [`cumulus/parachain-template`](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).
//!
//! The rollout process for Async Backing has three phases. Phases 1 and 2 below put new infrastructure
//! in place. Then we can simply turn on async backing in phase 3. But first, some pre-reqs and context
//! to set the stage.
//! ## Async Backing Prerequisites
//!
//! For more contextual information about asynchronous backing, see
//! [this page](https://wiki.polkadot.network/docs/learn-async-backing).
//!
//! Pull the latest version of Cumulus for use with your parachain. It contains necessary changes for
//! async backing compatibility. Latest on master branch of
//! [Polkadot-SDK](https://github.com/paritytech/polkadot-sdk) is currently sufficient. Any 2024 release
//! will work as well.
//!
//! ## Async Backing Terminology and Parameters
//!
//! Time for a bit of context before we get started. The following concepts will aid in demystifying the
//! collator side of Async Backing and establish a basic understanding of the changes being made:
//!
//! - **Unincluded segment** - From the perspective of a parachain block under construction, the
//!   unincluded segment describes a chain of recent block ancestors which have yet to be included on
//!   the relay chain. The ability to build new blocks on top of the unincluded segment rather than on
//!   top of blocks freshly included in the relay chain is the core of asynchronous backing.
//! - **Capacity** - The maximum size of the unincluded segment. The longer this is, the farther ahead a
//!   parachain can work, producing new candidates before the ancestors of those candidates have been
//!   seen as included on-chain. Practically, a capacity of 2-3 is sufficient to realize the full
//!   benefits of asynchronous backing, at least until the release of elastic scaling.
//! - **Velocity** - The base rate at which a parachain should produce blocks. A velocity of 1 indicates
//!   that 1 parachain block should be produced per relay chain block. In order to fill the unincluded
//!   segment with candidates, collators may build up to `Velocity + 1` candidates per aura slot while
//!   there is remaining capacity. When elastic scaling has been released velocities greater than 1 will
//!   be supported.
//! - **AllowMultipleBlocksPerSlot** - If this is `true`, Aura will allow slots to stay the same across
//!   sequential parablocks. Otherwise the slot number must increase with each block. To fill the
//!   unincluded segment as described above we need this to be `true`.
//! - **FixedVelocityConsensusHook** - This is a variety of `ConsensusHook` intended to be passed to
//!   `parachain-system` as part of its `Config`. It is triggered on initialization of a new runtime. An
//!   instance of `FixedVelocityConsensusHook` is defined with both a fixed capacity and velocity. It
//!   aborts the runtime early if either capacity or velocity is exceeded, as the collator shouldn’t be
//!   creating additional blocks in that case.
//! - **AsyncBackingParams.max_candidate_depth** - This parameter determines the maximum unincluded
//!   segment depth the relay chain will support. Candidates sent to validators which exceed
//!   `max_candidate_depth` will be ignored. `Capacity`, as mentioned above, should not exceed
//!   `max_candidate_depth`.
//! - **AsyncBackingParams.allowed_ancestry_len** - Each parachain block candidate has a `relay_parent`
//!   from which its execution and validation context is derived. Before async backing the
//!   `relay_parent` for a candidate not yet backed was required to be the fresh head of a fork. With
//!   async backing we can relax this requirement. Instead we set a conservative maximum age in blocks
//!   for the `relay_parent`s of candidates in the unincluded segment. This age, `allowed_ancestry_len`
//!   lives on the relay chain and is queried by parachains when deciding which block to build on top
//!   of.
//! - **Lookahead Collator** - A collator for Aura that looks ahead of the most recently included
//!   parachain block when determining what to build upon. This collator also builds additional blocks
//!   when the maximum backlog is not saturated. The size of the backlog is determined by invoking the
//!   AuraUnincludedSegmentApi. If that runtime API is not supported, this assumes a maximum backlog
//!   size of 1.
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
//! <div class="warning">`scheduling_lookahead` must be set to 2, otherwise parachain block times will degrade to worse than with sync backing!</div>
//! 
//!  
//! 
//! 
//!
//! ## Phase 1 - Update Parachain Runtime
//!
//! This phase involves configuring your parachain’s runtime to make use of async backing system.
//!
//! 1. Establish constants for `capacity` and `velocity` and set both of them to 1 in
//!    `/runtime/src/lib.rs`.
//!
//! 2. Establish a constant relay chain slot duration measured in milliseconds equal to `6000` in
//!    `/runtime/src/lib.rs`.
//!
//!    ```text
//!    //!/ Maximum number of blocks simultaneously accepted by the Runtime, not yet included into the
//!    //!/ relay chain.
//!    pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 1;
//!    //!/ How many parachain blocks are processed by the relay chain per parent. Limits the number of
//!    //!/ blocks authored per slot.
//!    pub const BLOCK_PROCESSING_VELOCITY: u32 = 1;
//!    //!/ Relay chain slot duration, in milliseconds.
//!    pub const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;
//!    ```
//!
//! 3. Establish constants `MILLISECS_PER_BLOCK` and `SLOT_DURATION` if not already present in
//!    `/runtime/src/lib.rs`.
//!
//!    ```text
//!
//!    //!/ BLOCKSkkhasd will be produced at a minimum duration defined by `SLOT_DURATION`.
//!    //!/ `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
//!    //!/ up by `pallet_aura` to implement `fn slot_duration()`.
//!    //!/
//!    //!/ Change this to adjust the block time.
//!    pub const MILLISECS_PER_BLOCK: u64 = 12000;
//!    pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
//!    ```
//!
//! 4. Configure `cumulus_pallet_parachain_system` in `runtime/src/lib.rs`
//!
//!    - Define a `FixedVelocityConsensusHook` using our capacity, velocity, and relay slot duration
//!      constants. Use this to set the parachain system `ConsensusHook` property.
//!
//!    ```text
//!    impl cumulus_pallet_parachain_system::Config for Runtime {
//!    	type RuntimeEvent = RuntimeEvent;
//!    	type OnSystemEvent = ();
//!    	type SelfParaId = parachain_info::Pallet<Runtime>;
//!    	type OutboundXcmpMessageSource = XcmpQueue;
//!    	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
//!    	type ReservedDmpWeight = ReservedDmpWeight;
//!    	type XcmpMessageHandler = XcmpQueue;
//!    	type ReservedXcmpWeight = ReservedXcmpWeight;
//!    	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
//!     
//!    	type ConsensusHook = ConsensusHook;
//!    	type WeightInfo = weights::cumulus_pallet_parachain_system::WeightInfo<Runtime>;
//!    }
//!    //! highlight-start
//!    type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
//!    	Runtime,
//!    	RELAY_CHAIN_SLOT_DURATION_MILLIS,
//!    	BLOCK_PROCESSING_VELOCITY,
//!    	UNINCLUDED_SEGMENT_CAPACITY,
//!    >;
//!    //! highlight-end
//!    ```
//!
//!    - Set the parachain system property `CheckAssociatedRelayNumber` to
//!      `RelayNumberMonotonicallyIncreases`
//!
//!    ```text
//!    impl cumulus_pallet_parachain_system::Config for Runtime {
//!    	type RuntimeEvent = RuntimeEvent;
//!    	type OnSystemEvent = ();
//!    	type SelfParaId = parachain_info::Pallet<Runtime>;
//!    	type OutboundXcmpMessageSource = XcmpQueue;
//!    	type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
//!    	type ReservedDmpWeight = ReservedDmpWeight;
//!    	type XcmpMessageHandler = XcmpQueue;
//!    	type ReservedXcmpWeight = ReservedXcmpWeight;
//!     
//!    	type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
//!    	type ConsensusHook = ConsensusHook;
//!    	type WeightInfo = weights::cumulus_pallet_parachain_system::WeightInfo<Runtime>;
//!    }
//!    type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
//!    	Runtime,
//!    	RELAY_CHAIN_SLOT_DURATION_MILLIS,
//!    	BLOCK_PROCESSING_VELOCITY,
//!    	UNINCLUDED_SEGMENT_CAPACITY,
//!    >;
//!    ```
//!
//! 5. Configure `pallet_aura` in `runtime/src/lib.rs`
//!
//!    - Set `AllowMultipleBlocksPerSlot` to `false` (don't worry, we will set it to `true` when we
//!      activate async backing in phase 3).
//!    - Define `pallet_aura::SlotDuration` using our constant `SLOT_DURATION`
//!
//!    ```text
//!    impl pallet_aura::Config for Runtime {
//!    	type AuthorityId = AuraId;
//!    	type DisabledValidators = ();
//!    	type MaxAuthorities = ConstU32<100_000>;
//!     //! highlight-start
//!    	type AllowMultipleBlocksPerSlot = ConstBool<false>;
//!    	#[cfg(feature = "experimental")]
//!    	type SlotDuration = ConstU64<SLOT_DURATION>;
//!     //! highlight-end
//!    }
//!    ```
//!
//! 6. Update `aura_api::SlotDuration()` to match the constant `SLOT_DURATION`
//!
//!    ```text
//!    impl_runtime_apis! {
//!    	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
//!    		fn slot_duration() -> sp_consensus_aura::SlotDuration {
//!             
//!    			sp_consensus_aura::SlotDuration::from_millis(SLOT_DURATION)
//!    		}
//!
//!    		fn authorities() -> Vec<AuraId> {
//!    			Aura::authorities().into_inner()
//!    		}
//!    	}
//!    ```
//!
//! 7. Implement the `AuraUnincludedSegmentApi`, which allows the collator client to query its runtime
//!    to determine whether it should author a block.
//!
//!    - Add the dependency `cumulus-primitives-aura` to the `runtime/Cargo.toml` file for your runtime
//!
//! ```text
//! cumulus-pallet-aura-ext = { path = "../../../../pallets/aura-ext", default-features = false }
//! cumulus-pallet-parachain-system = { path = "../../../../pallets/parachain-system", default-features = false, features = ["parameterized-consensus-hook"] }
//! cumulus-pallet-session-benchmarking = { path = "../../../../pallets/session-benchmarking", default-features = false }
//! cumulus-pallet-xcm = { path = "../../../../pallets/xcm", default-features = false }
//! cumulus-pallet-xcmp-queue = { path = "../../../../pallets/xcmp-queue", default-features = false, features = ["bridging"] }
//! cumulus-primitives-aura = { path = "../../../../primitives/aura", default-features = false }
//! ```
//!
//! - In the same file, add `"cumulus-primitives-aura/std",` to the `std` feature.
//!
//! - Inside the `impl_runtime_apis!` block for your runtime, implement the `AuraUnincludedSegmentApi`
//!   as shown below.
//!
//! ```text
//! impl cumulus_primitives_aura::AuraUnincludedSegmentApi<Block> for Runtime {
//! 	fn can_build_upon(
//! 		included_hash: <Block as BlockT>::Hash,
//! 		slot: cumulus_primitives_aura::Slot,
//! 	) -> bool {
//! 		ConsensusHook::can_build_upon(included_hash, slot)
//! 	}
//! }
//! ```
//!
//! **Note:** With a capacity of 1 we have an effective velocity of ½ even when velocity is configured
//! to some larger value. This is because capacity will be filled after a single block is produced and
//! will only be freed up after that block is included on the relay chain, which takes 2 relay blocks to
//! accomplish. Thus with capacity 1 and velocity 1 we get the customary 12 second parachain block time.
//!
//! 8. If your `runtime/src/lib.rs` provides a `CheckInherents` type to `register_validate_block`,
//!    remove it. `FixedVelocityConsensusHook` makes it unnecessary. The following example shows how
//!    `register_validate_block` should look after removing `CheckInherents`.
//!
//! ```text
//! cumulus_pallet_parachain_system::register_validate_block! {
//! 	Runtime = Runtime,
//! 	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
//! }
//! ```
//!
//! ## Phase 2 - Update Parachain Nodes
//!
//! This phase consists of plugging in the new lookahead collator node.
//!
//! 1. Import `cumulus_primitives_core::ValidationCode` to `node/src/service.rs`
//!
//! ```text
//! use cumulus_primitives_core::{
//! 	relay_chain::{CollatorPair, ValidationCode},
//! 	ParaId,
//! };
//! ```
//!
//! 2. In `node/src/service.rs`, modify `sc_service::spawn_tasks` to use a clone of `Backend` rather
//!    than the original
//!
//! ```text
//! sc_service::spawn_tasks(sc_service::SpawnTasksParams {
//! 	rpc_builder,
//! 	client: client.clone(),
//! 	transaction_pool: transaction_pool.clone(),
//! 	task_manager: &mut task_manager,
//! 	config: parachain_config,
//! 	keystore: params.keystore_container.keystore(),
//! 	backend: backend.clone(),
//! 	network: network.clone(),
//! 	sync_service: sync_service.clone(),
//! 	system_rpc_tx,
//! 	tx_handler_controller,
//! 	telemetry: telemetry.as_mut(),
//! })?;
//! ```
//!
//! 3. Add `backend` as a parameter to `start_consensus()` in `node/src/service.rs`
//!
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
//!
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
//!
//! ```text
//! use cumulus_client_consensus_aura::collators::lookahead::{self as aura, Params as AuraParams};
//! ```
//!
//! 5. In `start_consensus()` replace the `BasicAuraParams` struct with `AuraParams`
//!    - Change the struct type from `BasicAuraParams` to `AuraParams`
//!    - In the `para_client` field, pass in a cloned para client rather than the original
//!    - Add a `para_backend` parameter after `para_client`, passing in our para backend
//!    - Provide a `code_hash_provider` closure like that shown below
//!    - Increase `authoring_duration` from 500 milliseconds to 1500
//!
//! ```text
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
//! **Note:** Set `authoring_duration` to whatever you want, taking your own hardware into account. But
//! if the backer who should be slower than you due to reading from disk, times out at two seconds your
//! candidates will be rejected.
//!
//! 6. In `start_consensus()` replace `basic_aura::run` with `aura::run`
//!
//! ```text
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
//! 1. Configure `pallet_aura`, setting `AllowMultipleBlocksPerSlot` to true in `runtime/src/lib.rs`.
//!
//! ```text
//! impl pallet_aura::Config for Runtime {
//!     type AuthorityId = AuraId;
//!     type DisabledValidators = ();
//!     type MaxAuthorities = ConstU32<100_000>;
//!     
//!     type AllowMultipleBlocksPerSlot = ConstBool<true>;
//!     #[cfg(feature = "experimental")]
//!     type SlotDuration = ConstU64<SLOT_DURATION>;
//! }
//! ```
//!
//! 1. Increase the maximum `UNINCLUDED_SEGMENT_CAPACITY` in `runtime/src/lib.rs`.
//!
//! ```text
//! //!/ Maximum number of blocks simultaneously accepted by the Runtime, not yet included into the
//! //!/ relay chain.
//! pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
//! //!/ How many parachain blocks are processed by the relay chain per parent. Limits the number of
//! //!/ blocks authored per slot.
//! pub const BLOCK_PROCESSING_VELOCITY: u32 = 1;
//! ```
//!
//! 3. Decrease `MILLISECS_PER_BLOCK` to 6000.
//!
//! - Note: For a parachain which measures time in terms of its own block number rather than by relay
//!   block number it may be preferable to increase velocity. Changing block time may cause
//!   complications, requiring additional changes. See the section “Timing by Block Number”.
//!
//!   ```text
//!   //!/ This determines the average expected block time that we are targeting.
//!   //!/ Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
//!   //!/ `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
//!   //!/ up by `pallet_aura` to implement `fn slot_duration()`.
//!   //!/
//!   //!/ Change this to adjust the block time.
//!   pub const MILLISECS_PER_BLOCK: u64 = 6000;
//!   ```
//!
//! 4. Update `MAXIMUM_BLOCK_WEIGHT` to reflect the increased time available for block production.
//!
//! ```text
//! //!/ We allow for 2 seconds of compute with a 6 second average block.
//! pub const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
//!     WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
//!     cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
//! );
//! ```
//!
//! 5. Add a feature flagged alternative for `MinimumPeriod` in `pallet_timestamp`. The type should be
//!    `ConstU64<0>` with the feature flag experimental, and `ConstU64<{SLOT_DURATION / 2}>` without.
//!
//! ```text
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
//! With asynchronous backing it will be possible for parachains to opt for a block time of 6 seconds
//! rather than 12 seconds. But modifying block duration isn’t so simple for a parachain which was
//! measuring time in terms of its own block number. It could result in expected and actual time not
//! matching up, stalling the parachain.
//!
//! One strategy to deal with this issue is to instead rely on relay chain block numbers for timing.
//! Relay block number is kept track of by each parachain in `pallet-parachain-system` with the storage
//! value `LastRelayChainBlockNumber`. This value can be obtained and used wherever timing based on
//! block number is needed.