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
//! At present, with Asynchronous Backing enabled, a parachain can only include a block on the relay
//! chain every 6 seconds, irregardless of how many cores the parachain acquires. Elastic scaling
//! builds further on the 10x throughput increase of Async Backing, enabling collators to submit up
//! to 3 parachain blocks per relay chain block, resulting in a further 3x throughput increase.
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
//!    [More experiments](https://github.com/paritytech/polkadot-sdk/issues/4696) are being
//!    conducted in this space.
//! 3. **Trusted collator set.** The collator set needs to be trusted until there’s a mitigation
//!    that would prevent or deter multiple collators from submitting the same collation to multiple
//!    backing groups. A solution is being discussed
//!    [here](https://github.com/polkadot-fellows/RFCs/issues/92).
//! 4. **Fixed scaling.** For true elasticity, the parachain must be able to seamlessly acquire or
//!    sell coretime as the user demand grows and shrinks over time, in an automated manner. This is
//!    currently lacking - a parachain can only scale up or down by “manually” acquiring coretime.
//!    This is not in the scope of the relay chain functionality. Parachains can already start
//!    implementing such autoscaling, but we aim to provide a framework/examples for developing
//!    autoscaling strategies.
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
//! - Use a trusted single collator for maximum throughput.
//! - Ensure enough coretime is assigned to the parachain. For maximum throughput the upper bound is
//!   3 cores.
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
//!     - Remove the `overseer_handle` param (also remove the
//!     `OverseerHandle` type import if it’s not used elsewhere).
//!     - Rename `AuraParams` to `SlotBasedParams`, remove the `overseer_handle` field and add a
//!     `slot_drift` field with a   value of `Duration::from_secs(1)`.
//!     - Replace the single future returned by `aura::run` with the two futures returned by it and
//!     spawn them as separate tasks:
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", launch_slot_based_collator)]
//!
//! 3. In `start_parachain_node()` remove the `overseer_handle` param passed to `start_consensus`.
//!
//! ### Phase 2 - Activate fixed factor scaling in the runtime
//!
//! This phase consists of a couple of changes needed to be made to the parachain’s runtime in order
//! to utilise fixed factor scaling.
//!
//! First of all, you need to decide the upper limit to how many parachain blocks you need to
//! produce per relay chain block (in direct correlation with the number of acquired cores). This
//! should be either 1 (no scaling), 2 or 3. This is called the parachain velocity.
//!
//! If you configure a velocity which is different from the number of assigned cores, the measured
//! velocity in practice will be the minimum of these two.
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
//! 2. Set the `MILLISECS_PER_BLOCK` to the desired value.
//!
//!      ```ignore
//!      const MILLISECS_PER_BLOCK: u32 =
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
