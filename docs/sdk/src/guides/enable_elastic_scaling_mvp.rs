//! # Enable elastic scaling MVP for a parachain
//!
//! **This guide assumes full familiarity with Asynchronous Backing and its terminology, as defined
//! in <https://wiki.polkadot.network/docs/maintain-guides-async-backing>.
//! Furthermore, the parachain should have already been upgraded according to the guide.**
//!
//! ## Quick introduction to elastic scaling
//!
//! [Elastic scaling](https://polkadot.network/blog/elastic-scaling-streamling-growth-on-polkadot)
//! is a feature that will enable parachains to seamlessly scale up or down the amount of block
//! validated and backed in a single relay chain block, in order to keep up with demand.
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
//! Below are described the current limitations of the MVP:
//!
//! 1. **A parachain can use at most 3 cores at a time.** This limitation stems from the fact that
//!    every parablock has an execution timeout of 2 seconds and the relay chain block authoring
//!    takes 6 seconds. Therefore, assuming parablock authoring is sequential, a collator only has
//!    enough time to build 3 candidates in a relay chain slot.
//! 2. **Collator set must be of size 1 for consistently scaling beyond a core at full authorship
//!    duration of 2 seconds per block.** In practice, parablock authorship takes about the same
//!    amount of time as parablock import. Therefore, each collator must first import the previous
//!    block before authoring a new one. For a core count of 3, this would amount to 12 seconds and
//!    for a core count of 2, 8 seconds.
//! 3. **Trusted collator set.** The collator set needs to be trusted until there’s a mitigation
//!    that would prevent or deter multiple collators from submitting the same collation.
//! 4. **Fixed scaling.** For true elasticity, the parachain must be able to seamlessly acquire or
//!    sell coretime as the user demand grows and shrinks over time, in an automated manner. This is
//!    currently lacking - a parachain can only scale up or down by “manually” acquiring coretime.
//!    We call this fixed scaling.
//!
//! ## Using elastic scaling MVP
//!
//! ### Prerequisites
//!
//! - Ensure Asynchronous Backing is enabled on the network and you have enabled it on the parachain
//!   using [this guide](https://wiki.polkadot.network/docs/maintain-guides-async-backing).
//! - Ensure the `AsyncBackingParams.max_candidate_depth` value is configured as 6 on the relay
//!   chain (which will allow a max velocity of 3 for each parachain).
//! - Have a trusted collator set for the parachain (of size 1 for full throughput)
//! - The parachain has bought coretime for one or more cores (up to three) and is scheduled on the
//!   relay chain.
//! - Use the latest cumulus release, which includes the necessary elastic scaling changes
//!
//! The following steps assume using the cumulus parachain template.
//!
//! ### Phase 1 - Update Parachain Node
//!
//! This phase consists of plugging in the new slot-based collator node.
//!
//! 1. In `node/src/service.rs` import the slot based collator instead of the lookahead collator.
//!
//! ```rust
//! use cumulus_client_consensus_aura::collators::slot_based::{self as aura, Params as AuraParams};
//! ```
//!
//! 2. In `start_consensus()`
//!     - Remove the `overseer_handle` param (also remove the
//!     `OverseerHandle` type import if it’s not used elsewhere).
//!     - In `AuraParams`, remove the `sync_oracle` and `overseer_handle` fields and add a
//!     `slot_drift` field with a   value of `Duration::from_secs(1)`.
//!     - Replace the single future returned by `aura::run` with the two futures returned by it and
//!     spawn them as separate tasks:
//!      ```rust
//!      let (collation_future, block_builder_future) = aura::run::<
//!          Block,
//!          sp_consensus_aura::sr25519::AuthorityPair,
//!          _,
//!          _,
//!          _,
//!          _,
//!          _,
//!          _,
//!          _,
//!          _>(params);
//!      task_manager
//!          .spawn_essential_handle()
//!          .spawn("collation-task", None, collation_future);
//!      task_manager
//!          .spawn_essential_handle()
//!          .spawn("block-builder-task", None, block_builder_future);
//!     ```
//!
//! 3. In `start_parachain_node()` remove the `sync_service` and `overseer_handle` params passed to
//!    `start_consensus`
//!
//! ### Phase 2 - Activate fixed factor scaling in the runtime
//!
//! This phase consists of a couple of changes needed to be made to the parachain’s runtime in order
//! to utilise fixed factor scaling.
//!
//! First of all, you need to decide how many parachain blocks you need to produce per relay chain
//! block (in direct correlation with the number of acquired cores). This should be either 1 (no
//! scaling), 2 or 3. This is called the parachain velocity.
//!
//! If you configure a velocity which is different from the number of assigned cores, the measured
//! velocity in practice will be the minimum of these two.
//!
//! The chosen velocity should also be used to compute:
//! - The slot duration, by dividing the 6000 ms duration of the relay chain slot duration by the
//! velocity.
//! - The unincluded segment capacity, by multiplying the velocity with 2 and adding 1 to
//! it.
//!
//! Let’s assume a desired velocity of 3 parachain blocks per relay chain block. The needed changes
//! would all be done in `runtime/src/lib.rs`:
//!
//! 1. Increase the `BLOCK_PROCESSING_VELOCITY` to the desired value. In this example, 3.
//!
//!      ```rust
//!      const BLOCK_PROCESSING_VELOCITY: u32 = 3;
//!      ```
//!
//! 2. Decrease the `MILLISECS_PER_BLOCK` to the desired value. In this example, 2000.
//!
//!      ```rust
//!      const MILLISECS_PER_BLOCK: u32 = 2000;
//!      ```
//!     Note: for a parachain which measures time in terms of its own block number, changing block
//!     time may cause complications, requiring additional changes.  See the section ["Timing by
//!     block number" of the async backing guide](https://wiki.polkadot.network/docs/maintain-guides-async-backing#timing-by-block-number).
//!
//! 3. Increase the `UNINCLUDED_SEGMENT_CAPACITY` to the desired value. In this example, 7.
//!
//!     ```rust
//!     const UNINCLUDED_SEGMENT_CAPACITY: u32 = 7;
//!     ```
