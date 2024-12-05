//! # Enable elastic scaling MVP for a parachain
//!
//! <div class="warning">This guide assumes full familiarity with Asynchronous Backing and its
//! terminology, as defined in <a href="https://wiki.polkadot.network/docs/maintain-guides-async-backing">the Polkadot Wiki</a>.
//! Furthermore, the parachain should have already been upgraded according to the guide.</div>
//!
//! ## Quick introduction to elastic scaling
//!
//! [Elastic scaling](https://polkadot.network/blog/elastic-scaling-streamling-growth-on-polkadot)
//! is a feature that will enable parachains to seamlessly scale up/down the number of used cores.
//! This can be desirable in order to increase the compute or storage throughput of a parachain or
//! to lower the latency between a transaction being submitted and it getting built in a parachain
//! block.
//!
//! ## Current limitations of the MVP
//!
//! The full implementation of elastic scaling spans across the entire relay/parachain stack and is
//! still [work in progress](https://github.com/paritytech/polkadot-sdk/issues/1829).
//! The MVP is still considered experimental software, so stability is not guaranteed.
//! If you encounter any problems,
//! [please open an issue](https://github.com/paritytech/polkadot-sdk/issues).
//! Below are described the current limitations of the MVP:
//!
//! 1. **Limited core count**. Parachain block authoring is sequential, so the second block will
//!    start being built only after the previous block is imported. The current block production is
//!    capped at 2 seconds of execution. Therefore, assuming the full 2 seconds are used, a
//!    parachain can only utilise at most 3 cores in a relay chain slot of 6 seconds. If the full
//!    execution time is not being used, higher core counts can be achieved.
//! 2. **Single collator requirement for consistently scaling beyond a core at full authorship
//!    duration of 2 seconds per block.** Using the current implementation with multiple collators
//!    adds additional latency to the block production pipeline. Assuming block execution takes
//!    about the same as authorship, the additional overhead is equal the duration of the authorship
//!    plus the block announcement. Each collator must first import the previous block before
//!    authoring a new one, so it is clear that the highest throughput can be achieved using a
//!    single collator. Experiments show that the peak performance using more than one collator
//!    (measured up to 10 collators) is utilising 2 cores with authorship time of 1.3 seconds per
//!    block, which leaves 400ms for networking overhead. This would allow for 2.6 seconds of
//!    execution, compared to the 2 seconds async backing enabled.
//!    The development required for lifting this limitation is tracked by
//!    [this issue](https://github.com/paritytech/polkadot-sdk/issues/5190)
//! 2. **Fixed scaling.** For true elasticity, the parachain must be able to seamlessly acquire or
//!    sell coretime as the user demand grows and shrinks over time, in an automated manner. This is
//!    currently lacking - a parachain can only scale up or down by “manually” acquiring coretime.
//!    This is not in the scope of the relay chain functionality. Parachains can already start
//!    implementing such autoscaling, but we aim to provide a framework/examples for developing
//!    autoscaling strategies.
//!    Tracked by [this issue](https://github.com/paritytech/polkadot-sdk/issues/1487).
//!
//! Another hard limitation that is not envisioned to ever be lifted is that parachains which create
//! forks will generally not be able to utilise the full number of cores they acquire.
//!
//! ## Using elastic scaling MVP
//!
//! ### Prerequisites
//!
//! - Ensure Asynchronous Backing is enabled on the network and you have enabled it on the parachain
//!   using [`crate::guides::async_backing_guide`].
//! - Ensure the `AsyncBackingParams.max_candidate_depth` value is configured to a value that is at
//!   least double the maximum targeted parachain velocity. For example, if the parachain will build
//!   at most 3 candidates per relay chain block, the `max_candidate_depth` should be at least 6.
//! - Ensure enough coretime is assigned to the parachain. For maximum throughput the upper bound is
//!   3 cores.
//! - Ensure the `CandidateReceiptV2` node feature is enabled on the relay chain configuration (node
//!   feature bit number 3).
//!
//! <div class="warning">Phase 1 is NOT needed if using the <code>polkadot-parachain</code> or
//! <code>polkadot-omni-node</code> binary, or <code>polkadot-omni-node-lib</code> built from the
//! latest polkadot-sdk release! Simply pass the <code>--experimental-use-slot-based</code>
//! ([`polkadot_omni_node_lib::cli::Cli::experimental_use_slot_based`]) parameter to the command
//! line and jump to Phase 2.</div>
//!
//! The following steps assume using the cumulus parachain template.
//!
//! ### Phase 1 - (For custom parachain node) Update Parachain Node
//!
//! This assumes you are using
//! [the latest parachain template](https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain).
//!
//! This phase consists of plugging in the new slot-based collator.
//!
//! 1. In `node/src/service.rs` import the slot based collator instead of the lookahead collator.
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", slot_based_colator_import)]
//!
//! 2. In `start_consensus()`
//!     - Remove the `overseer_handle` and `relay_chain_slot_duration` params (also remove the
//!     `OverseerHandle` type import if it’s not used elsewhere).
//!     - Rename `AuraParams` to `SlotBasedParams`, remove the `overseer_handle` and
//!     `relay_chain_slot_duration` fields and add a `slot_drift` field with a value of
//!     `Duration::from_secs(1)`. Also add a `spawner` field initialized to
//!     `task_manager.spawn_handle()`.
//!     - Replace the `aura::run` with the `slot_based::run` call and remove the explicit task
//!       spawn:
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", launch_slot_based_collator)]
//!
//! 3. In `start_parachain_node()` remove the `overseer_handle` and `relay_chain_slot_duration`
//!    params passed to `start_consensus`.
//!
//! ### Phase 2 - Configure core selection policy in the parachain runtime
//!
//! With the addition of [RFC-103](https://polkadot-fellows.github.io/RFCs/approved/0103-introduce-core-index-commitment.html),
//! the parachain runtime has the responsibility of selecting which of the assigned cores to build
//! on. It does so by implementing the `SelectCore` trait.
#![doc = docify::embed!("../../cumulus/pallets/parachain-system/src/lib.rs", SelectCore)]
//!
//! For the vast majority of use cases though, you will not need to implement a custom core
//! selector. There are two core selection policies to choose from (without implementing your own)
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
//! type SelectCore = SelectCore<Runtime>;
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
//! 	}
//! ```
//!
//! ### Phase 3 - Configure fixed factor scaling in the runtime
//!
//! This phase consists of a couple of changes needed to be made to the parachain’s runtime in order
//! to utilise fixed factor scaling.
//!
//! First of all, you need to decide the upper limit to how many parachain blocks you need to
//! produce per relay chain block (in direct correlation with the number of acquired cores). This
//! should be either 1 (no scaling), 2 or 3. This is called the parachain velocity.
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
