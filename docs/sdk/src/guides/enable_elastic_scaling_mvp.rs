//! # Enable elastic scaling MVP for a parachain
//!
//! terminology, as defined in <a href="https://wiki.polkadot.network/docs/maintain-guides-async-backing">the Polkadot Wiki</a>.
//!
//!
//! is a feature that will enable parachains to seamlessly scale up/down the number of used cores.
//! to lower the latency between a transaction being submitted and it getting built in a parachain
//!
//! chain every 6 seconds, irregardless of how many cores the parachain acquires. Elastic scaling
//! to 3 parachain blocks per relay chain block, resulting in a further 3x throughput increase.
//!
//!
//! still [`work in progress`].
//! If you encounter any problems,
//! Below are described the current limitations of the MVP:
//!
//!    start being built only after the previous block is imported. The current block production is
//!    parachain can only utilise at most 3 cores in a relay chain slot of 6 seconds. If the full
//! 2. **Single collator requirement for consistently scaling beyond a core at full authorship
//!    adds additional latency to the block production pipeline. Assuming block execution takes
//!    plus the block announcement. Each collator must first import the previous block before
//!    single collator. Experiments show that the peak performance using more than one collator
//!    block, which leaves 400ms for networking overhead. This would allow for 2.6 seconds of
//!    [`More experiments`] are being
//! 3. **Trusted collator set.** The collator set needs to be trusted until there’s a mitigation
//!    backing groups. A solution is being discussed
//! 4. **Fixed scaling.** For true elasticity, the parachain must be able to seamlessly acquire or
//!    currently lacking - a parachain can only scale up or down by “manually” acquiring coretime.
//!    implementing such autoscaling, but we aim to provide a framework/examples for developing
//!
//! forks will generally not be able to utilise the full number of cores they acquire.
//!
//!
//!
//!   using [`async_backing_guide`].
//!   least double the maximum targeted parachain velocity. For example, if the parachain will build
//! - Use a trusted single collator for maximum throughput.
//!   3 cores.
//!
//! <code>polkadot-omni-node</code> binary, or <code>polkadot-omni-node-lib</code> built from the
//! ([`polkadot_omni_node_lib::cli::Cli::experimental_use_slot_based`]) parameter to the command
//!
//!
//!
//! [`the latest parachain template`].
//!
//!
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", slot_based_colator_import)]
//!
//!     - Remove the `overseer_handle` param (also remove the
//!     - Rename `AuraParams` to `SlotBasedParams`, remove the `overseer_handle` field and add a
//!     - Replace the single future returned by `aura::run` with the two futures returned by it and
#![doc = docify::embed!("../../cumulus/polkadot-omni-node/lib/src/nodes/aura.rs", launch_slot_based_collator)]
//!
//!
//!
//! to utilise fixed factor scaling.
//!
//! produce per relay chain block (in direct correlation with the number of acquired cores). This
//!
//! velocity in practice will be the minimum of these two.
//!
//! - The slot duration, by dividing the 6000 ms duration of the relay chain slot duration by the
//! - The unincluded segment capacity, by multiplying the velocity with 2 and adding 1 to
//!
//! changes would all be done in `runtime/src/lib.rs`:
//!
//!    desired value. In this example, 3.
//!
//!      const MAX_BLOCK_PROCESSING_VELOCITY: u32 = 3;
//!
//!
//!      const MILLISECS_PER_BLOCK: u32 =
//!      ```
//!     time may cause complications, requiring additional changes.  See here more information:
//!
//!
//!     const UNINCLUDED_SEGMENT_CAPACITY: u32 = 2 * MAX_BLOCK_PROCESSING_VELOCITY + 1;

// Link References
// [`async_backing_guide`]: crate::guides::async_backing_guide#timing-by-block-number

// [`Elastic scaling`]: https://polkadot.network/blog/elastic-scaling-streamling-growth-on-polkadot
// [`More experiments`]: https://github.com/paritytech/polkadot-sdk/issues/4696
// [`async_backing_guide`]: async_backing_guide#timing-by-block-number
// [`here`]: https://github.com/polkadot-fellows/RFCs/issues/92
// [`please open an issue`]: https://github.com/paritytech/polkadot-sdk/issues
// [`the latest parachain template`]: https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain
// [`work in progress`]: https://github.com/paritytech/polkadot-sdk/issues/1829
