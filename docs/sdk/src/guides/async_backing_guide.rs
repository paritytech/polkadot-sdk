//! # Upgrade Parachain for Asynchronous Backing Compatibility
//!
//! backing process is synchronous where parablocks can only be built on the latest Relay Chain
//! pipelines of multiple pending parablocks. This parallel block generation increases efficiency
//! on [`the Polkadot Wiki.`]
//!
//! > the
//! The rollout process for Async Backing has three phases. Phases 1 and 2 below put new
//!
//!
//! configuration contains the following three parameters (especially when testing locally e.g. with
//!
//! "async_backing_params": {
//!     "allowed_ancestry_len": 2
//! "scheduling_lookahead": 2
//!
//! block times will degrade to worse than with sync backing!</div>
//!
//!
//! async backing system.
//!
//!    runtime.
//!    `6000` in the runtime.
//! // Maximum number of blocks simultaneously accepted by the Runtime, not yet included into the
//! pub const UNINCLUDED_SEGMENT_CAPACITY: u32 = 1;
//! // blocks authored per slot.
//! // Relay chain slot duration, in milliseconds.
//! ```
//!
//!    runtime.
//! // `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
//! //
//! pub const MILLISECS_PER_BLOCK: u64 = 12000;
//! ```
//!
//!
//! constants. Use this to set the parachain system `ConsensusHook` property.
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", ConsensusHook)]
//! impl cumulus_pallet_parachain_system::Config for Runtime {
//!     type ConsensusHook = ConsensusHook;
//! }
//! - Set the parachain system property `CheckAssociatedRelayNumber` to
//! ```ignore
//! 	..
//! 	..
//! ```
//!
//!
//! activate async backing in phase 3).
//!
//! ```ignore
//! 	..
//! 	#[cfg(feature = "experimental")]
//! 	..
//! ```
//!
//!    the constant `SLOT_DURATION`
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_slot_duration)]
//!
//!    runtime to determine whether it should author a block.
//!
//!      runtime
//! ..
//! ..
//!
//!
//!   `cumulus_primitives_aura::AuraUnincludedSegmentApi` as shown below.
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_can_build_upon)]
//!
//! configured to some larger value. This is because capacity will be filled after a single block is
//! 2 relay blocks to accomplish. Thus with capacity 1 and velocity 1 we get the customary 12 second
//!
//!    remove it. `FixedVelocityConsensusHook` makes it unnecessary. The following example shows how
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", register_validate_block)]
//!
//!
//!
//!
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", cumulus_primitives)]
//!
//!    than the original
//! sc_service::spawn_tasks(sc_service::SpawnTasksParams {
//!     backend: backend.clone(),
//! })?;
//!
//! ```text
//!     ..
//!     ..
//! ```ignore
//!   start_consensus(
//!     backend.clone(),
//!    )?;
//! ```
//!
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", lookahead_collator)]
//!
//!    - Change the struct type from `BasicAuraParams` to `AuraParams`
//!    - Add a `para_backend` parameter after `para_client`, passing in our para backend
//!    - Increase `authoring_duration` from 500 milliseconds to 2000
//! let params = AuraParams {
//!     para_client: client.clone(),
//!     ..
//!         client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
//!     ..
//!     ..
//! ```
//!
//! But if the backer who should be slower than you due to reading from disk, times out at two
//!
//! ```ignore
//! aura::run::<Block, sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _, _, _, _, _>(
//! );
//! ```
//!
//!
//!
//!    `runtime/src/lib.rs`.
#![doc = docify::embed!("../../templates/parachain/runtime/src/configs/mod.rs", aura_config)]
//!
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", async_backing_params)]
//!
//!
//!   relay block number it may be preferable to increase velocity. Changing block time may cause
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", block_times)]
//!
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", max_block_weight)]
//!
//!    be `ConstU64<0>` with the feature flag experimental, and `ConstU64<{SLOT_DURATION / 2}>`
//! ```ignore
//!     ..
//!     type MinimumPeriod = ConstU64<0>;
//!     type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
//! }
//!
//!
//! seconds rather than 12 seconds. But modifying block duration isn’t so simple for a parachain
//! actual time not matching up, stalling the parachain.
//!
//! Relay block number is kept track of by each parachain in `pallet-parachain-system` with the
//! based on block number is needed.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

// [`parachain template`]: https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain
// [`the Polkadot Wiki.`]: https://wiki.polkadot.network/docs/maintain-guides-async-backing
