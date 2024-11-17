#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", ConsensusHook)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_slot_duration)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/apis.rs", impl_can_build_upon)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", register_validate_block)]
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", cumulus_primitives)]
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", lookahead_collator)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/configs/mod.rs", aura_config)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", async_backing_params)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", block_times)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", max_block_weight)]

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]


// [`parachain template`]: https://github.com/paritytech/polkadot-sdk/tree/master/templates/parachain
// [`the Polkadot Wiki.`]: https://wiki.polkadot.network/docs/maintain-guides-async-backing