//! # Enable elastic scaling for a parachain
//!
//! <div class="warning">This guide assumes full familiarity with Asynchronous Backing and its
//! terminology, as defined in <a href="https://wiki.polkadot.network/docs/maintain-guides-async-backing">the Polkadot Wiki</a>.
//! Furthermore, the parachain should have already been upgraded according to the guide.</div>
//!
//! ## Quick introduction to elastic scaling
//!
//! [Elastic scaling](https://polkadot.network/blog/elastic-scaling-streamling-growth-on-polkadot)
//! is a feature that enables parachains to seamlessly scale up/down the number of used cores.
//! This can be used to increase the available compute and bandwidth resources of a parachain or
//! to lower the transaction inclusion latency by decreasing block time.
//!
//! ## Performance characteristics and constraints
//!
//! Elastic scaling is still considered experimental software, so stability is not guaranteed.
//! If you encounter any problems,
//! [please open an issue](https://github.com/paritytech/polkadot-sdk/issues).
//! Below are described the constraints and performance characteristics of the implementation:
//!
//! 1. **Bounded compute throughput**. Each parachain block gets at most 2 seconds of execution on
//!    the relay chain. Therefore, assuming the full 2 seconds are used, a parachain can only
//!    utilise at most 3 cores in a relay chain slot of 6 seconds. If the full execution time is not
//!    being used or if all collators are able to author blocks faster than the reference hardware,
//!    higher core counts can be achieved.
//! 2. **Sequential block execution**. Each collator must import the previous block before authoring
//!    a new one. At present this happens sequentially, which limits the maximum compute throughput when
//!    using multiple collators. To briefly explain the reasoning: first, the previous collator spends
//!    2 seconds building the block and announces it. The next collator fetches and executes it, wasting
//!    2 seconds plus the block fetching duration out of its 2 second slot. Therefore, the next collator
//!    cannot build a subsequent block in due time and ends up authoring a fork, which defeats the purpose
//!    of elastic scaling. The highest throughput can therefore be achieved with a single collator but
//!    this should obviously only be used for testing purposes, due to the clear lack of decentralisation
//!    and resilience. In other words, to fully utilise the cores, the following formula needs to be
//!    satisfied: `2 * authorship duration + network overheads <= slot time`. For example, you can use
//!    2 cores with authorship time of 1.3 seconds per block, which leaves 400ms for networking overhead.
//!    This would allow for 2.6 seconds of execution, compared to the 2 seconds async backing enabled.
//!    If block authoring duration is low and you attempt to use elastic scaling for achieving low latency
//!    or increasing storage throughput, this is not a problem. Developments required for streamlining
//!    block production are tracked by [this issue](https://github.com/paritytech/polkadot-sdk/issues/5190).
//! 3. **Lack of out-of-the-box automated scaling.** For true elasticity, the parachain must be able
//!    to seamlessly acquire or sell coretime as the user demand grows and shrinks over time, in an
//!    automated manner. This is currently lacking - a parachain can only scale up or down by
//!    implementing some off-chain solution for managing the core time resources.
//!    This is not in the scope of the relay chain functionality. Parachains can already start
//!    implementing such autoscaling, but we aim to provide a framework/examples for developing
//!    autoscaling strategies.
//!    Tracked by [this issue](https://github.com/paritytech/polkadot-sdk/issues/1487).
//!    An in-progress external implementation by RegionX can be found [here](https://github.com/RegionX-Labs/On-Demand).
//!
//! Another important constraint is that when a parachain forks, the throughput decreases and
//! latency increases because the number of blocks backed per relay chain block goes down.
//!
//! ## Using elastic scaling
//!
//! [Here](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/zombienet-sdk-tests/tests/elastic_scaling/slot_based_12cores.rs)
//! is a zombienet test which exercises 500ms parachain blocks, which you can modify to test your
//! parachain after going through the required phases below.
//!
//! ### Prerequisites
//!
//! - Ensure Asynchronous Backing (6-second blocks) has been enabled on the parachain using
//!   [`crate::guides::async_backing_guide`].
//! - Ensure the `AsyncBackingParams.max_candidate_depth` value is configured to a value that is at
//!   least double the maximum targeted parachain velocity. For example, if the parachain will build
//!   at most 3 candidates per relay chain block, the `max_candidate_depth` should be at least 6.
//! - Ensure enough coretime is assigned to the parachain.
//! - Ensure the `CandidateReceiptV2` node feature is enabled on the relay chain configuration (node
//!   feature bit number 3).
//!
//! <div class="warning">Phase 1 is NOT needed if using the <code>polkadot-parachain</code> or
//! <code>polkadot-omni-node</code> binary, or <code>polkadot-omni-node-lib</code> built from the
//! latest polkadot-sdk release! Simply pass the <code>--experimental-use-slot-based</code>
//! ([`polkadot_omni_node_lib::cli::Cli::experimental_use_slot_based`]) parameter to the command
//! line and jump to Phase 2.</div>
//!
//! ### Phase 1 - (For custom parachain node) Update Parachain Node
//!
//! This assumes you are using
//! [the latest parachain template](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).
//!
//! This phase consists of plugging in the new slot-based collator, which is a requirement for
//! elastic scaling.
//!
//! 1. In `node/src/service.rs` import the slot based collator instead of the lookahead collator, as
//!    well as the `SlotBasedBlockImport` and `SlotBasedBlockImportHandle`.
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", slot_based_colator_import)]
//!
//! 2. Modify the `ParachainBlockImport` and `Service` type definitions:
//! ```ignore
//! type ParachainBlockImport = TParachainBlockImport<
//! 	    Block,
//! 	    SlotBasedBlockImport<Block, Arc<ParachainClient>, ParachainClient>,
//! 	    ParachainBackend,
//! >;
//! ```
//!
//! ```ignore
//! pub type Service = PartialComponents<
//!     ParachainClient,
//!     ParachainBackend,
//!     (),
//!     sc_consensus::DefaultImportQueue<Block>,
//!     sc_transaction_pool::TransactionPoolHandle<Block, ParachainClient>,
//!     (
//!         ParachainBlockImport,
//!         SlotBasedBlockImportHandle<Block>,
//!         Option<Telemetry>,
//!         Option<TelemetryWorkerHandle>,
//!     ),
//! >;
//! ```
//!
//! 3. In `new_partial()`:
//!     - Instantiate the `SlotBasedBlockImport` and pass the returned `block_import` value to
//!       `ParachainBlockImport::new` and the returned `slot_based_handle` to the `other` field of
//!       the `PartialComponents` struct.
//!      
//!      ```ignore
//!      let (block_import, slot_based_handle) = SlotBasedBlockImport::new(
//!          client.clone(),
//!          client.clone()
//!      );
//!      let block_import = ParachainBlockImport::new(block_import.clone(), backend.clone());
//!      ```
//!
//!      ```ignore
//!      Ok(PartialComponents {
//! 		backend,
//! 		client,
//! 		import_queue,
//! 		keystore_container,
//! 		task_manager,
//! 		transaction_pool,
//! 		select_chain: (),
//! 		other: (block_import, slot_based_handle, telemetry, telemetry_worker_handle),
//! 	 })
//!      ```
//!
//! 4. In `start_consensus()`:
//!     - Remove the `overseer_handle` and `relay_chain_slot_duration` params (also remove the
//!     `OverseerHandle` type import if it’s not used elsewhere).
//!     - Add a new parameter for the block import handle:
//!     `block_import_handle: SlotBasedBlockImportHandle<Block>`
//!     - Rename `AuraParams` to `SlotBasedParams`, remove the `overseer_handle` and
//!     `relay_chain_slot_duration` fields and add a `slot_drift` field with a value of
//!     `Duration::from_secs(1)`. Also add a `spawner` field initialized to
//!     `task_manager.spawn_handle()` and pass in the `block_import_handle` param.
//!     - (Optional): You may need to customise the `authoring_duration` field of `SlotBasedParams`
//!      if using more than 3 cores. The authoring duration generally needs to be equal to the
//!      parachain slot duration.
//!     - Replace the `aura::run` with the `slot_based::run` call and remove the explicit task
//!       spawn:
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", launch_slot_based_collator)]
//!
//! 3. In `start_parachain_node()`, destructure `slot_based_handle` from `params.other`. Remove the
//!    `overseer_handle` and `relay_chain_slot_duration` params passed to `start_consensus` and pass
//!    in the `slot_based_handle`.
//!
//! ### Phase 2 - Configure core selection policy in the parachain runtime
//!
//! [RFC-103](https://polkadot-fellows.github.io/RFCs/approved/0103-introduce-core-index-commitment.html) enables
//! parachain runtimes to constrain the execution of each block to a specified core, ensuring better
//! security and liveness, which is mandatory for launching in production. More details are
//! described in the RFC. To make use of this feature, the `SelectCore` trait needs to be
//! implemented.
#![doc = docify::embed!("../../cumulus/pallets/parachain-system/src/lib.rs", SelectCore)]
//!
//! For the vast majority of use cases, you will not need to implement a custom core
//! selector. There are two pre-defined core selection policies to choose from
//! `DefaultCoreSelector` and `LookaheadCoreSelector`.
//!
//! - The `DefaultCoreSelector` implements a round-robin selection on the cores that can be
//! occupied by the parachain at the very next relay parent. This is the equivalent to what all
//! parachains on production networks have been using so far.
//!
//! - The `LookaheadCoreSelector` also does a round robin on the assigned cores, but not those that
//! can be occupied at the very next relay parent. Instead, it uses the ones after. In other words,
//! the collator gets more time to build and advertise a collation for an assignment. This makes no
//! difference in practice if the parachain is continuously scheduled on the cores. This policy is
//! especially desirable for parachains that are sharing a core or that use on-demand coretime.
//!
//! In your /runtime/src/lib.rs, define a `SelectCore` type and use this to set the `SelectCore`
//! property (overwrite it with the chosen policy type):
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", default_select_core)]
//! ```ignore
//! impl cumulus_pallet_parachain_system::Config for Runtime {
//! ...
//!     type SelectCore = SelectCore<Runtime>;
//! ...
//! }
//! ```
//!
//! Next, we need to implement the `GetCoreSelector` runtime API. In the `impl_runtime_apis` block
//! for your runtime, add the following code:
//!
//! ```ignore
//! impl cumulus_primitives_core::GetCoreSelectorApi<Block> for Runtime {
//! 		fn core_selector() -> (cumulus_primitives_core::CoreSelector, cumulus_primitives_core::ClaimQueueOffset) {
//! 			ParachainSystem::core_selector()
//! 		}
//! }
//! ```
//!
//! ### Phase 3 - Configure maximum scaling factor in the runtime
//!
//! *A sample test parachain runtime which has compile-time features for configuring elastic scaling
//! can be found [here](https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/test/runtime/src/lib.rs)*
//!
//! First of all, you need to decide the upper limit to how many parachain blocks you need to
//! produce per relay chain block (in direct correlation with the number of acquired cores).
//! This is called the parachain velocity.
//!
//! <div class="warning">If you configure a velocity which is different from the number of assigned
//! cores, the measured velocity in practice will be the minimum of these two. However, be mindful
//! that if the velocity is higher than the number of assigned cores, it's possible that
//! <a href="https://github.com/paritytech/polkadot-sdk/issues/6667"> only a subset of the collator set will be authoring blocks.</a></div>
//!
//! The chosen velocity will also be used to compute:
//! - The slot duration, by dividing the 6000 ms duration of the relay chain slot duration by the
//! velocity.
//! - The unincluded segment capacity, by multiplying the velocity with 2 and adding 1 to
//! it.
//!
//! Let’s assume a desired maximum velocity of 3 parachain blocks per relay chain block. The needed
//! changes would all be done in `runtime/src/lib.rs`:
//!
//! 1. Rename `BLOCK_PROCESSING_VELOCITY` to `MAX_BLOCK_PROCESSING_VELOCITY` and increase it to the
//!    desired value. In this example, 3.
//!
//!      ```ignore
//!      const MAX_BLOCK_PROCESSING_VELOCITY: u32 = 3;
//!      ```
//!
//! 2. Set the `MILLI_SECS_PER_BLOCK` to the desired value.
//!
//!      ```ignore
//!      const MILLI_SECS_PER_BLOCK: u32 =
//!          RELAY_CHAIN_SLOT_DURATION_MILLIS / MAX_BLOCK_PROCESSING_VELOCITY;
//!      ```
//!     Note: for a parachain which measures time in terms of its own block number, changing block
//!     time may cause complications, requiring additional changes.  See here more information:
//!     [`crate::guides::async_backing_guide#timing-by-block-number`].
//!
//! 3. Increase the `UNINCLUDED_SEGMENT_CAPACITY` to the desired value.
//!
//!     ```ignore
//!     const UNINCLUDED_SEGMENT_CAPACITY: u32 = 2 * MAX_BLOCK_PROCESSING_VELOCITY + 1;
//!     ```
