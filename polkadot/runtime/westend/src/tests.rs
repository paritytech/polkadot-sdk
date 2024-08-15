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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Tests for the Westend Runtime Configuration

use std::collections::HashSet;

use crate::*;
use frame_support::traits::WhitelistedStorageKeys;
use sp_core::hexdisplay::HexDisplay;

#[test]
fn remove_keys_weight_is_sensible() {
	use polkadot_runtime_common::crowdloan::WeightInfo;
	let max_weight = <Runtime as crowdloan::Config>::WeightInfo::refund(RemoveKeysLimit::get());
	// Max remove keys limit should be no more than half the total block weight.
	assert!((max_weight * 2).all_lt(BlockWeights::get().max_block));
}

#[test]
fn sample_size_is_sensible() {
	use polkadot_runtime_common::auctions::WeightInfo;
	// Need to clean up all samples at the end of an auction.
	let samples: BlockNumber = EndingPeriod::get() / SampleLength::get();
	let max_weight: frame_support::weights::Weight =
		RocksDbWeight::get().reads_writes(samples.into(), samples.into());
	// Max sample cleanup should be no more than half the total block weight.
	assert!((max_weight * 2).all_lt(BlockWeights::get().max_block));
	assert!((<Runtime as auctions::Config>::WeightInfo::on_initialize() * 2)
		.all_lt(BlockWeights::get().max_block));
}

#[test]
fn call_size() {
	RuntimeCall::assert_size_under(256);
}

#[test]
fn sanity_check_teleport_assets_weight() {
	// This test sanity checks that at least 50 teleports can exist in a block.
	// Usually when XCM runs into an issue, it will return a weight of `Weight::MAX`,
	// so this test will certainly ensure that this problem does not occur.
	use frame_support::dispatch::GetDispatchInfo;
	let weight = pallet_xcm::Call::<Runtime>::limited_teleport_assets {
		dest: Box::new(Here.into()),
		beneficiary: Box::new(Here.into()),
		assets: Box::new((Here, 200_000).into()),
		fee_asset_item: 0,
		weight_limit: Unlimited,
	}
	.get_dispatch_info()
	.weight;

	assert!((weight * 50).all_lt(BlockWeights::get().max_block));
}

#[test]
fn check_whitelist() {
	let whitelist: HashSet<String> = AllPalletsWithSystem::whitelisted_storage_keys()
		.iter()
		.map(|e| HexDisplay::from(&e.key).to_string())
		.collect();

	// Block number
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac"));
	// Total issuance
	assert!(whitelist.contains("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80"));
	// Execution phase
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a"));
	// Event count
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850"));
	// System events
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7"));
	// Configuration ActiveConfig
	assert!(whitelist.contains("06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385"));
	// XcmPallet VersionDiscoveryQueue
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d194a222ba0333561192e474c59ed8e30e1"));
	// XcmPallet SafeXcmVersion
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d196323ae84c43568be0d1394d5d0d522c4"));
}

#[test]
fn check_treasury_pallet_id() {
	assert_eq!(
		<Treasury as frame_support::traits::PalletInfoAccess>::index() as u8,
		westend_runtime_constants::TREASURY_PALLET_ID
	);
}

#[cfg(all(test, feature = "try-runtime"))]
mod remote_tests {
	use super::*;
	use frame_try_runtime::{runtime_decl_for_try_runtime::TryRuntime, UpgradeCheckSelect};
	use remote_externalities::{
		Builder, Mode, OfflineConfig, OnlineConfig, SnapshotConfig, Transport,
	};
	use std::env::var;

	#[tokio::test]
	async fn run_migrations() {
		if var("RUN_MIGRATION_TESTS").is_err() {
			return;
		}

		sp_tracing::try_init_simple();
		let transport: Transport =
			var("WS").unwrap_or("wss://westend-rpc.polkadot.io:443".to_string()).into();
		let maybe_state_snapshot: Option<SnapshotConfig> = var("SNAP").map(|s| s.into()).ok();
		let mut ext = Builder::<Block>::default()
			.mode(if let Some(state_snapshot) = maybe_state_snapshot {
				Mode::OfflineOrElseOnline(
					OfflineConfig { state_snapshot: state_snapshot.clone() },
					OnlineConfig {
						transport,
						state_snapshot: Some(state_snapshot),
						..Default::default()
					},
				)
			} else {
				Mode::Online(OnlineConfig { transport, ..Default::default() })
			})
			.build()
			.await
			.unwrap();
		ext.execute_with(|| Runtime::on_runtime_upgrade(UpgradeCheckSelect::PreAndPost));
	}

	#[tokio::test]
	async fn delegate_stake_migration() {
		// Intended to be run only manually.
		if var("RUN_MIGRATION_TESTS").is_err() {
			return;
		}
		use frame_support::assert_ok;
		sp_tracing::try_init_simple();

		let transport: Transport = var("WS").unwrap_or("ws://127.0.0.1:9900".to_string()).into();
		let maybe_state_snapshot: Option<SnapshotConfig> = var("SNAP").map(|s| s.into()).ok();
		let mut ext = Builder::<Block>::default()
			.mode(if let Some(state_snapshot) = maybe_state_snapshot {
				Mode::OfflineOrElseOnline(
					OfflineConfig { state_snapshot: state_snapshot.clone() },
					OnlineConfig {
						transport,
						state_snapshot: Some(state_snapshot),
						pallets: vec![
							"staking".into(),
							"system".into(),
							"balances".into(),
							"nomination-pools".into(),
							"delegated-staking".into(),
						],
						..Default::default()
					},
				)
			} else {
				Mode::Online(OnlineConfig { transport, ..Default::default() })
			})
			.build()
			.await
			.unwrap();
		ext.execute_with(|| {
			// create an account with some balance
			let alice = AccountId::from([1u8; 32]);
			use frame_support::traits::Currency;
			let _ = Balances::deposit_creating(&alice, 100_000 * UNITS);

			// iterate over all pools
			pallet_nomination_pools::BondedPools::<Runtime>::iter_keys().for_each(|k| {
				if pallet_nomination_pools::Pallet::<Runtime>::api_pool_needs_delegate_migration(k)
				{
					assert_ok!(
						pallet_nomination_pools::Pallet::<Runtime>::migrate_pool_to_delegate_stake(
							RuntimeOrigin::signed(alice.clone()).into(),
							k,
						)
					);
				}
			});

			// member migration stats
			let mut success = 0;
			let mut direct_stakers = 0;
			let mut unexpected_errors = 0;

			// iterate over all pool members
			pallet_nomination_pools::PoolMembers::<Runtime>::iter_keys().for_each(|k| {
				if pallet_nomination_pools::Pallet::<Runtime>::api_member_needs_delegate_migration(
					k.clone(),
				) {
					// reasons migrations can fail:
					let is_direct_staker = pallet_staking::Bonded::<Runtime>::contains_key(&k);

					let migration = pallet_nomination_pools::Pallet::<Runtime>::migrate_delegation(
						RuntimeOrigin::signed(alice.clone()).into(),
						sp_runtime::MultiAddress::Id(k.clone()),
					);

					if is_direct_staker {
						// if the member is a direct staker, the migration should fail until pool
						// member unstakes all funds from pallet-staking.
						direct_stakers += 1;
						assert_eq!(
							migration.unwrap_err(),
							pallet_delegated_staking::Error::<Runtime>::AlreadyStaking.into()
						);
					} else if migration.is_err() {
						unexpected_errors += 1;
						log::error!(target: "remote_test", "Unexpected error {:?} while migrating {:?}", migration.unwrap_err(), k);
					} else {
						success += 1;
					}
				}
			});

			log::info!(
				target: "remote_test",
				"Migration stats: success: {}, direct_stakers: {}, unexpected_errors: {}",
				success,
				direct_stakers,
				unexpected_errors
			);
		});
	}
}
