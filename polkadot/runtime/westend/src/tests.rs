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

use crate::{xcm_config::LocationConverter, *};
use approx::assert_relative_eq;
use frame_support::traits::WhitelistedStorageKeys;
use pallet_staking::EraPayout;
use sp_core::{crypto::Ss58Codec, hexdisplay::HexDisplay};
use sp_keyring::AccountKeyring::Alice;
use xcm_runtime_apis::conversions::LocationToAccountHelper;

const MILLISECONDS_PER_HOUR: u64 = 60 * 60 * 1000;

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
	.call_weight;

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

#[test]
fn location_conversion_works() {
	// the purpose of hardcoded values is to catch an unintended location conversion logic change.
	struct TestCase {
		description: &'static str,
		location: Location,
		expected_account_id_str: &'static str,
	}

	let test_cases = vec![
		// DescribeTerminus
		TestCase {
			description: "DescribeTerminus Child",
			location: Location::new(0, [Parachain(1111)]),
			expected_account_id_str: "5Ec4AhP4h37t7TFsAZ4HhFq6k92usAAJDUC3ADSZ4H4Acru3",
		},
		// DescribePalletTerminal
		TestCase {
			description: "DescribePalletTerminal Child",
			location: Location::new(0, [Parachain(1111), PalletInstance(50)]),
			expected_account_id_str: "5FjEBrKn3STAFsZpQF4jzwxUYHNGnNgzdZqSQfTzeJ82XKp6",
		},
		// DescribeAccountId32Terminal
		TestCase {
			description: "DescribeAccountId32Terminal Child",
			location: Location::new(
				0,
				[Parachain(1111), AccountId32 { network: None, id: AccountId::from(Alice).into() }],
			),
			expected_account_id_str: "5EEMro9RRDpne4jn9TuD7cTB6Amv1raVZ3xspSkqb2BF3FJH",
		},
		// DescribeAccountKey20Terminal
		TestCase {
			description: "DescribeAccountKey20Terminal Child",
			location: Location::new(
				0,
				[Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
			),
			expected_account_id_str: "5HohjXdjs6afcYcgHHSstkrtGfxgfGKsnZ1jtewBpFiGu4DL",
		},
		// DescribeTreasuryVoiceTerminal
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Child",
			location: Location::new(
				0,
				[Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5GenE4vJgHvwYVcD6b4nBvH5HNY4pzpVHWoqwFpNMFT7a2oX",
		},
		// DescribeBodyTerminal
		TestCase {
			description: "DescribeBodyTerminal Child",
			location: Location::new(
				0,
				[Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5DPgGBFTTYm1dGbtB1VWHJ3T3ScvdrskGGx6vSJZNP1WNStV",
		},
	];

	for tc in test_cases {
		let expected =
			AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

		let got = LocationToAccountHelper::<AccountId, LocationConverter>::convert_location(
			tc.location.into(),
		)
		.unwrap();

		assert_eq!(got, expected, "{}", tc.description);
	}
}

#[test]
fn staking_inflation_correct_single_era() {
	let (to_stakers, to_treasury) = super::EraPayout::era_payout(
		123, // ignored
		456, // ignored
		MILLISECONDS_PER_HOUR,
	);

	assert_relative_eq!(to_stakers as f64, (4_046 * CENTS) as f64, max_relative = 0.01);
	assert_relative_eq!(to_treasury as f64, (714 * CENTS) as f64, max_relative = 0.01);
	// Total per hour is ~47.6 WND
	assert_relative_eq!(
		(to_stakers as f64 + to_treasury as f64),
		(4_760 * CENTS) as f64,
		max_relative = 0.001
	);
}

#[test]
fn staking_inflation_correct_longer_era() {
	// Twice the era duration means twice the emission:
	let (to_stakers, to_treasury) = super::EraPayout::era_payout(
		123, // ignored
		456, // ignored
		2 * MILLISECONDS_PER_HOUR,
	);

	assert_relative_eq!(to_stakers as f64, (4_046 * CENTS) as f64 * 2.0, max_relative = 0.001);
	assert_relative_eq!(to_treasury as f64, (714 * CENTS) as f64 * 2.0, max_relative = 0.001);
}

#[test]
fn staking_inflation_correct_whole_year() {
	let (to_stakers, to_treasury) = super::EraPayout::era_payout(
		123,                                        // ignored
		456,                                        // ignored
		(36525 * 24 * MILLISECONDS_PER_HOUR) / 100, // 1 year
	);

	// Our yearly emissions is about 417k WND:
	let yearly_emission = 417_307 * UNITS;
	assert_relative_eq!(
		to_stakers as f64 + to_treasury as f64,
		yearly_emission as f64,
		max_relative = 0.001
	);

	assert_relative_eq!(to_stakers as f64, yearly_emission as f64 * 0.85, max_relative = 0.001);
	assert_relative_eq!(to_treasury as f64, yearly_emission as f64 * 0.15, max_relative = 0.001);
}

// 10 years into the future, our values do not overflow.
#[test]
fn staking_inflation_correct_not_overflow() {
	let (to_stakers, to_treasury) = super::EraPayout::era_payout(
		123,                                       // ignored
		456,                                       // ignored
		(36525 * 24 * MILLISECONDS_PER_HOUR) / 10, // 10 years
	);
	let initial_ti: i128 = 5_216_342_402_773_185_773;
	let projected_total_issuance = (to_stakers as i128 + to_treasury as i128) + initial_ti;

	// In 2034, there will be about 9.39 million WND in existence.
	assert_relative_eq!(
		projected_total_issuance as f64,
		(9_390_000 * UNITS) as f64,
		max_relative = 0.001
	);
}

// Print percent per year, just as convenience.
#[test]
fn staking_inflation_correct_print_percent() {
	let (to_stakers, to_treasury) = super::EraPayout::era_payout(
		123,                                        // ignored
		456,                                        // ignored
		(36525 * 24 * MILLISECONDS_PER_HOUR) / 100, // 1 year
	);
	let yearly_emission = to_stakers + to_treasury;
	let mut ti: i128 = 5_216_342_402_773_185_773;

	for y in 0..10 {
		let new_ti = ti + yearly_emission as i128;
		let inflation = 100.0 * (new_ti - ti) as f64 / ti as f64;
		println!("Year {y} inflation: {inflation}%");
		ti = new_ti;

		assert!(inflation <= 8.0 && inflation > 2.0, "sanity check");
	}
}
