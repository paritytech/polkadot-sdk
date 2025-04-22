// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::porting_prelude::*;

use asset_hub_polkadot_runtime::{
	AhMigrator, Block as AssetHubBlock, Runtime as AssetHub, RuntimeEvent as AhRuntimeEvent,
};
use codec::Decode;
use cumulus_primitives_core::{
	AggregateMessageOrigin as ParachainMessageOrigin, InboundDownwardMessage, ParaId,
};
use frame_support::traits::{EnqueueMessage, OnFinalize, OnInitialize};
use pallet_rc_migrator::{
	DmpDataMessageCounts as RcDmpDataMessageCounts, MigrationStage as RcMigrationStage,
	MigrationStageOf as RcMigrationStageOf, RcMigrationStage as RcMigrationStageStorage,
};
use polkadot_primitives::UpwardMessage;
use polkadot_runtime::{
	Block as PolkadotBlock, RcMigrator, Runtime as Polkadot, RuntimeEvent as RcRuntimeEvent,
};
use remote_externalities::{Builder, Mode, OfflineConfig, RemoteExternalities};
use runtime_parachains::{
	dmp::DownwardMessageQueues,
	inclusion::{AggregateMessageOrigin as RcMessageOrigin, UmpQueueId},
};
use sp_runtime::{BoundedVec, Perbill};
use std::str::FromStr;

pub const AH_PARA_ID: ParaId = ParaId::new(1000);
const LOG_RC: &str = "runtime::relay";
const LOG_AH: &str = "runtime::asset-hub";

/// Load Relay and AH externalities in parallel.
pub async fn load_externalities(
) -> Option<(RemoteExternalities<PolkadotBlock>, RemoteExternalities<AssetHubBlock>)> {
	let (rc, ah) = tokio::try_join!(
		tokio::spawn(async { remote_ext_test_setup::<PolkadotBlock>("SNAP_RC").await }),
		tokio::spawn(async { remote_ext_test_setup::<AssetHubBlock>("SNAP_AH").await })
	)
	.ok()?;
	Some((rc?, ah?))
}

pub async fn remote_ext_test_setup<Block: sp_runtime::traits::Block>(
	env: &str,
) -> Option<RemoteExternalities<Block>> {
	sp_tracing::try_init_simple();
	let snap = std::env::var(env).ok()?;
	let abs = std::path::absolute(snap.clone());

	let ext = Builder::<Block>::default()
		.mode(Mode::Offline(OfflineConfig { state_snapshot: snap.clone().into() }))
		.build()
		.await
		.map_err(|e| {
			eprintln!("Could not load from snapshot: {:?}: {:?}", abs, e);
		})
		.unwrap();

	Some(ext)
}

pub fn next_block_rc() {
	let past = frame_system::Pallet::<Polkadot>::block_number();
	let now = past + 1;
	log::debug!(target: LOG_RC, "Next block: {:?}", now);
	frame_system::Pallet::<Polkadot>::set_block_number(now);
	frame_system::Pallet::<Polkadot>::reset_events();
	let weight = <polkadot_runtime::MessageQueue as OnInitialize<_>>::on_initialize(now);
	let weight = <RcMigrator as OnInitialize<_>>::on_initialize(now).saturating_add(weight);
	<polkadot_runtime::MessageQueue as OnFinalize<_>>::on_finalize(now);
	<RcMigrator as OnFinalize<_>>::on_finalize(now);

	let events = frame_system::Pallet::<Polkadot>::events();
	assert!(
		!events.iter().any(|record| {
			if matches!(
				record.event,
				RcRuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: false,
					..
				})
			) {
				log::error!(target: LOG_AH, "Message processing error: {:?}", events);
				true
			} else {
				false
			}
		}),
		"unexpected xcm message processing failure",
	);

	let limit = <Polkadot as frame_system::Config>::BlockWeights::get().max_block;
	assert!(
		weight.all_lte(Perbill::from_percent(80) * limit),
		"Weight exceeded 80% of limit: {:?}, limit: {:?}",
		weight,
		limit
	);
}

pub fn next_block_ah() {
	let past = frame_system::Pallet::<AssetHub>::block_number();
	let now = past + 1;
	log::debug!(target: LOG_AH, "Next block: {:?}", now);
	frame_system::Pallet::<AssetHub>::set_block_number(now);
	frame_system::Pallet::<AssetHub>::reset_events();
	let weight = <asset_hub_polkadot_runtime::MessageQueue as OnInitialize<_>>::on_initialize(now);
	let weight = <AhMigrator as OnInitialize<_>>::on_initialize(now).saturating_add(weight);
	<asset_hub_polkadot_runtime::MessageQueue as OnFinalize<_>>::on_finalize(now);
	<AhMigrator as OnFinalize<_>>::on_finalize(now);

	let events = frame_system::Pallet::<AssetHub>::events();
	assert!(
		!events.iter().any(|record| {
			if matches!(
				record.event,
				AhRuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: false,
					..
				})
			) {
				log::error!(target: LOG_AH, "Message processing error: {:?}", events);
				true
			} else {
				false
			}
		}),
		"unexpected xcm message processing failure",
	);

	let limit = <AssetHub as frame_system::Config>::BlockWeights::get().max_block;
	assert!(
		weight.all_lte(Perbill::from_percent(80) * limit),
		"Weight exceeded 80% of limit: {:?}, limit: {:?}",
		weight,
		limit
	);
}

/// Enqueue DMP messages on the parachain side.
///
/// This bypasses `set_validation_data` and `enqueue_inbound_downward_messages` by just directly
/// enqueuing them.
pub fn enqueue_dmp(msgs: Vec<InboundDownwardMessage>) {
	log::info!("Enqueuing {} DMP messages", msgs.len());
	for msg in msgs {
		// Sanity check that we can decode it
		if let Err(e) =
			xcm::VersionedXcm::<asset_hub_polkadot_runtime::RuntimeCall>::decode(&mut &msg.msg[..])
		{
			panic!("Failed to decode XCM: 0x{}: {:?}", hex::encode(&msg.msg), e);
		}

		let bounded_msg: BoundedVec<u8, _> = msg.msg.try_into().expect("DMP message too big");
		asset_hub_polkadot_runtime::MessageQueue::enqueue_message(
			bounded_msg.as_bounded_slice(),
			ParachainMessageOrigin::Parent,
		);
	}
}

/// Enqueue DMP messages on the parachain side.
pub fn enqueue_ump(msgs: Vec<UpwardMessage>) {
	for msg in msgs {
		if let Err(e) = xcm::VersionedXcm::<polkadot_runtime::RuntimeCall>::decode(&mut &msg[..]) {
			panic!("Failed to decode XCM: 0x{}: {:?}", hex::encode(&msg), e);
		}

		let bounded_msg: BoundedVec<u8, _> = msg.try_into().expect("UMP message too big");
		polkadot_runtime::MessageQueue::enqueue_message(
			bounded_msg.as_bounded_slice(),
			RcMessageOrigin::Ump(UmpQueueId::Para(AH_PARA_ID)),
		);
	}
}

// Sets the initial migration stage on the Relay Chain.
//
// If the `START_STAGE` environment variable is set, it will be used to set the initial migration
// stage. Otherwise, the `AccountsMigrationInit` stage will be set bypassing the `Scheduled` stage.
// The `Scheduled` stage is tested separately by the `scheduled_migration_works` test.
pub fn set_initial_migration_stage(
	relay_chain: &mut RemoteExternalities<PolkadotBlock>,
) -> RcMigrationStageOf<Polkadot> {
	let stage = relay_chain.execute_with(|| {
		let stage = if let Ok(stage) = std::env::var("START_STAGE") {
			log::info!("Setting start stage: {:?}", &stage);
			RcMigrationStage::from_str(&stage).expect("Invalid start stage")
		} else {
			RcMigrationStage::Scheduled { block_number: 0u32.into() }
		};
		RcMigrationStageStorage::<Polkadot>::put(stage.clone());
		stage
	});
	relay_chain.commit_all().unwrap();
	stage
}

// Migrates the pallet out of the Relay Chain and returns the corresponding Payload.
//
// Sends DMP messages with pallet migration data from relay chain to asset hub. The output includes
// both the DMP messages sent from the relay chain to asset hub, which will be used to perform the
// migration, and the relay chain payload, which will be used to check the correctness of the
// migration process.
pub fn rc_migrate(
	relay_chain: &mut RemoteExternalities<PolkadotBlock>,
) -> Vec<InboundDownwardMessage> {
	// AH parachain ID
	let para_id = ParaId::from(1000);

	// Simulate relay blocks and grab the DMP messages
	let dmp_messages = relay_chain.execute_with(|| {
		let mut dmps = Vec::new();

		// Loop until no more DMPs are added and we had at least 1
		loop {
			next_block_rc();

			// Bypass the unconfirmed DMP messages limit since we do not send the messages to the AH
			// on every RC block.
			let (sent, _) = RcDmpDataMessageCounts::<Polkadot>::get();
			RcDmpDataMessageCounts::<Polkadot>::put((sent, sent));

			let new_dmps = DownwardMessageQueues::<Polkadot>::take(para_id);
			dmps.extend(new_dmps);

			match RcMigrationStageStorage::<Polkadot>::get() {
				RcMigrationStage::Initializing => {
					log::info!("Migration initializing, waiting for AH signal");
					break dmps;
				},
				RcMigrationStage::MigrationDone => {
					log::info!("Migration done");
					break dmps;
				},
				_ => (),
			}
		}
	});
	relay_chain.commit_all().unwrap();
	log::info!("Num of RC->AH DMP messages: {}", dmp_messages.len());
	dmp_messages
}

// Migrates the pallet into Asset Hub.
//
// Processes all the pending DMP messages in the AH message queue to complete the pallet
// migration. Uses the relay chain pre-migration payload to check the correctness of the
// migration once completed.
pub fn ah_migrate(
	asset_hub: &mut RemoteExternalities<AssetHubBlock>,
	dmp_messages: Vec<InboundDownwardMessage>,
) {
	// Inject the DMP messages into the Asset Hub
	asset_hub.execute_with(|| {
		let mut fp =
			asset_hub_polkadot_runtime::MessageQueue::footprint(ParachainMessageOrigin::Parent);
		enqueue_dmp(dmp_messages);

		// Loop until no more DMPs are queued
		loop {
			let new_fp =
				asset_hub_polkadot_runtime::MessageQueue::footprint(ParachainMessageOrigin::Parent);
			if fp == new_fp {
				log::info!("AH DMP messages left: {}", fp.storage.count);
				break;
			}
			fp = new_fp;

			log::debug!("AH DMP messages left: {}", fp.storage.count);
			next_block_ah();
		}

		// NOTE that the DMP queue is probably not empty because the snapshot that we use
		// contains some overweight ones.
		// TODO compare with the number of messages before the migration
	});
	asset_hub.commit_all().unwrap();
}
