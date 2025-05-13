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

//! Rust integration for the Asset Hub Migration.
//!
//! This test calls `on_initialize` on the RC and on AH alternately and forwards DMP messages.
//!
//! Create snapshots in the root dir:
//!
//! ```
//! try-runtime create-snapshot --uri wss://sys.ibp.network:443/statemint ah-polkadot.snap
//! try-runtime create-snapshot --uri wss://try-runtime.polkadot.io:443 polkadot.snap
//! ```
//!
//! Run with:
//!
//! ```
//! SNAP_RC="../../polkadot.snap" SNAP_AH="../../ah-polkadot.snap" RUST_LOG="info" ct polkadot-integration-tests-ahm -r on_initialize_works -- --nocapture
//! ```

use crate::porting_prelude::*;

use super::{
	checks::SanityChecks,
	mock::*,
	multisig_still_work::MultisigStillWork,
	proxy::{ProxyBasicWorks, ProxyWhaleWatching},
};
use asset_hub_polkadot_runtime::Runtime as AssetHub;
use cumulus_pallet_parachain_system::PendingUpwardMessages;
use cumulus_primitives_core::{BlockT, InboundDownwardMessage, Junction, Location, ParaId};
use frame_support::{
	assert_err,
	traits::{
		fungible::Inspect, schedule::DispatchTime, Currency, ExistenceRequirement,
		ReservableCurrency,
	},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_ah_migrator::{
	proxy::ProxyBasicChecks, types::AhMigrationCheck, AhMigrationStage as AhMigrationStageStorage,
	MigrationStage as AhMigrationStage,
};
use pallet_rc_migrator::{
	types::RcMigrationCheck, MigrationStage as RcMigrationStage,
	RcMigrationStage as RcMigrationStageStorage,
};
use polkadot_primitives::UpwardMessage;
use polkadot_runtime::{Block as PolkadotBlock, RcMigrator, Runtime as Polkadot};
use polkadot_runtime_common::{paras_registrar, slots as pallet_slots};
use remote_externalities::RemoteExternalities;
use runtime_parachains::dmp::DownwardMessageQueues;
use sp_core::crypto::Ss58Codec;
use sp_runtime::{AccountId32, DispatchError, TokenError};
use std::{
	collections::{BTreeMap, VecDeque},
	str::FromStr,
};
use xcm::latest::*;
use xcm_emulator::{assert_ok, ConvertLocation, WeightMeter};

type RcChecks = (
	SanityChecks,
	pallet_rc_migrator::accounts::AccountsMigrator<Polkadot>,
	pallet_rc_migrator::preimage::PreimageChunkMigrator<Polkadot>,
	pallet_rc_migrator::preimage::PreimageRequestStatusMigrator<Polkadot>,
	pallet_rc_migrator::preimage::PreimageLegacyRequestStatusMigrator<Polkadot>,
	pallet_rc_migrator::indices::IndicesMigrator<Polkadot>,
	pallet_rc_migrator::vesting::VestingMigrator<Polkadot>,
	pallet_rc_migrator::proxy::ProxyProxiesMigrator<Polkadot>,
	pallet_rc_migrator::staking::bags_list::BagsListMigrator<Polkadot>,
	pallet_rc_migrator::staking::fast_unstake::FastUnstakeMigrator<Polkadot>,
	pallet_rc_migrator::conviction_voting::ConvictionVotingMigrator<Polkadot>,
	pallet_rc_migrator::asset_rate::AssetRateMigrator<Polkadot>,
	pallet_rc_migrator::scheduler::SchedulerMigrator<Polkadot>,
	pallet_rc_migrator::staking::nom_pools::NomPoolsMigrator<Polkadot>,
	pallet_rc_migrator::referenda::ReferendaMigrator<Polkadot>,
	RcPolkadotChecks,
	// other checks go here (if available on Polkadot, Kusama and Westend)
	ProxyBasicWorks,
	MultisigStillWork,
);

// Checks that are specific to Polkadot, and not available on other chains (like Westend)
#[cfg(feature = "ahm-polkadot")]
pub type RcPolkadotChecks = (
	pallet_rc_migrator::bounties::BountiesMigrator<Polkadot>,
	pallet_rc_migrator::treasury::TreasuryMigrator<Polkadot>,
	pallet_rc_migrator::claims::ClaimsMigrator<Polkadot>,
	pallet_rc_migrator::crowdloan::CrowdloanMigrator<Polkadot>,
	ProxyWhaleWatching,
);

#[cfg(not(feature = "ahm-polkadot"))]
pub type RcPolkadotChecks = ();

type AhChecks = (
	SanityChecks,
	pallet_rc_migrator::accounts::AccountsMigrator<AssetHub>,
	pallet_rc_migrator::preimage::PreimageChunkMigrator<AssetHub>,
	pallet_rc_migrator::preimage::PreimageRequestStatusMigrator<AssetHub>,
	pallet_rc_migrator::preimage::PreimageLegacyRequestStatusMigrator<AssetHub>,
	pallet_rc_migrator::indices::IndicesMigrator<AssetHub>,
	pallet_rc_migrator::vesting::VestingMigrator<AssetHub>,
	pallet_ah_migrator::proxy::ProxyBasicChecks<
		AssetHub,
		<Polkadot as pallet_proxy::Config>::ProxyType,
	>,
	pallet_rc_migrator::staking::bags_list::BagsListMigrator<AssetHub>,
	pallet_rc_migrator::staking::fast_unstake::FastUnstakeMigrator<AssetHub>,
	pallet_rc_migrator::conviction_voting::ConvictionVotingMigrator<AssetHub>,
	pallet_rc_migrator::asset_rate::AssetRateMigrator<AssetHub>,
	pallet_rc_migrator::scheduler::SchedulerMigrator<AssetHub>,
	pallet_rc_migrator::staking::nom_pools::NomPoolsMigrator<AssetHub>,
	pallet_rc_migrator::referenda::ReferendaMigrator<AssetHub>,
	AhPolkadotChecks,
	// other checks go here (if available on Polkadot, Kusama and Westend)
	ProxyBasicWorks,
	MultisigStillWork,
);

// Checks that are specific to Asset Hub Migration on Polkadot, and not available on other chains
// (like AH Westend)
#[cfg(feature = "ahm-polkadot")]
pub type AhPolkadotChecks = (
	pallet_rc_migrator::bounties::BountiesMigrator<AssetHub>,
	pallet_rc_migrator::treasury::TreasuryMigrator<AssetHub>,
	pallet_rc_migrator::claims::ClaimsMigrator<AssetHub>,
	pallet_rc_migrator::crowdloan::CrowdloanMigrator<AssetHub>,
	ProxyWhaleWatching,
);

#[cfg(not(feature = "ahm-polkadot"))]
pub type AhPolkadotChecks = ();

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pallet_migration_works() {
	let (mut rc, mut ah) = load_externalities().await.unwrap();

	// Set the initial migration stage from env var if set.
	set_initial_migration_stage(&mut rc);

	// Pre-checks on the Relay
	let rc_pre = run_check(|| RcChecks::pre_check(), &mut rc);

	// Pre-checks on the Asset Hub
	let ah_pre = run_check(|| AhChecks::pre_check(rc_pre.clone().unwrap()), &mut ah);

	// Run relay chain, sends start signal to AH
	let dmp_messages = rc_migrate(&mut rc);
	// AH process start signal, send back ack
	ah_migrate(&mut ah, dmp_messages);
	// no upward messaging support in this test yet, just manually advance the stage
	rc.execute_with(|| {
		RcMigrationStageStorage::<Polkadot>::put(RcMigrationStage::Starting);
	});
	rc.commit_all().unwrap();

	// Migrate the Relay Chain
	let dmp_messages = rc_migrate(&mut rc);

	// Post-checks on the Relay
	run_check(|| RcChecks::post_check(rc_pre.clone().unwrap()), &mut rc);

	// Migrate the Asset Hub
	ah_migrate(&mut ah, dmp_messages);

	ah.execute_with(|| {
		assert_eq!(
			pallet_ah_migrator::AhMigrationStage::<AssetHub>::get(),
			pallet_ah_migrator::MigrationStage::MigrationDone
		);
	});

	// Post-checks on the Asset Hub
	run_check(|| AhChecks::post_check(rc_pre.unwrap(), ah_pre.unwrap()), &mut ah);
}

fn run_check<R, B: BlockT>(f: impl FnOnce() -> R, ext: &mut RemoteExternalities<B>) -> Option<R> {
	if std::env::var("START_STAGE").is_err() {
		Some(ext.execute_with(|| f()))
	} else {
		None
	}
}

#[cfg(not(feature = "ahm-westend"))] // No auctions on Westend
#[tokio::test]
async fn num_leases_to_ending_block_works_simple() {
	let mut rc = remote_ext_test_setup::<PolkadotBlock>("SNAP_RC").await.unwrap();
	let f = |now: BlockNumberFor<Polkadot>, num_leases: u32| {
		frame_system::Pallet::<Polkadot>::set_block_number(now);
		pallet_rc_migrator::crowdloan::num_leases_to_ending_block::<Polkadot>(num_leases)
	};

	rc.execute_with(|| {
		let p = <Polkadot as pallet_slots::Config>::LeasePeriod::get();
		let o = <Polkadot as pallet_slots::Config>::LeaseOffset::get();

		// Sanity check:
		assert!(f(1000, 0).is_err());
		assert!(f(1000, 10).is_err());
		// Overflow check:
		assert!(f(o, u32::MAX).is_err());

		// In period 0:
		assert_eq!(f(o, 0), Ok(o));
		assert_eq!(f(o, 1), Ok(o + p));
		assert_eq!(f(o, 2), Ok(o + 2 * p));

		// In period 1:
		assert_eq!(f(o + p, 0), Ok(o + p));
		assert_eq!(f(o + p, 1), Ok(o + 2 * p));
		assert_eq!(f(o + p, 2), Ok(o + 3 * p));

		// In period 19 with 5 remaining:
		assert_eq!(f(o + 19 * p, 1), Ok(o + 20 * p));
		assert_eq!(f(o + 19 * p, 5), Ok(o + 24 * p));
	});
}

#[test]
fn sovereign_account_translation() {
	let good_cases = [
		(
			// para 2094 account https://polkadot.subscan.io/account/13YMK2dzLWfnGZXSLuAxgZbBiNMHLfnPZ8itzwXryJ9FcWsE
			"13YMK2dzLWfnGZXSLuAxgZbBiNMHLfnPZ8itzwXryJ9FcWsE",
			// on ah (different account id) https://assethub-polkadot.subscan.io/account/13cKp88oRErgQAFatu83oCvzxr2b45qVcnNLFu4Mr2ApU6ZC
			"13cKp88oRErgQAFatu83oCvzxr2b45qVcnNLFu4Mr2ApU6ZC",
		),
		(
			"13YMK2dsXbyC866w2tFM4vH52nRs3uTwac32jh1FNXZBXv18",
			"13cKp88gcLA6Fgq5atCSBZctHG7AmKX3eFgTzeXkFFakPWuo",
		),
	];

	for (rc_acc, ah_acc) in good_cases {
		let rc_acc = AccountId32::from_str(rc_acc).unwrap();
		let ah_acc = AccountId32::from_str(ah_acc).unwrap();

		let (translated, _para_id) = pallet_rc_migrator::accounts::AccountsMigrator::<Polkadot>::try_translate_rc_sovereign_to_ah(rc_acc).unwrap().unwrap();
		assert_eq!(translated, ah_acc);
	}

	let bad_cases = [
		"13yJaZUmhMDG91AftfdNeJm6hMVSL9Jq2gqiyFdhiJgXf6AY", // wrong prefix
		"13ddruDZgGbfVmbobzfNLV4momSgjkFnMXkfogizb4uEbHtQ", // "
		"13cF4T4kfi8VYw2nTZfkYkn9BjGpmRDsivYxFqGYUWkU8L2d", // "
		"13cKp88gcLA6Fgq5atCSBZctHG7AmKX3eFgTzeXkFFakPo6e", // last byte not 0
		"13cF4T4kfiJ39NqGh4DAZSMo6NuWT1fYfZzCo9f5HH8dUFBJ", // 7 byte not zero
		"13cKp88gcLA6Fgq5atCSBZctHGenFzUo3qmmReNVKzpnGvFg", // some center byte not zero
	];

	for rc_acc in bad_cases {
		let rc_acc = AccountId32::from_str(rc_acc).unwrap();

		let translated = pallet_rc_migrator::accounts::AccountsMigrator::<Polkadot>::try_translate_rc_sovereign_to_ah(rc_acc).unwrap();
		assert!(translated.is_none());
	}
}

/// For human consumption.
#[tokio::test]
async fn print_sovereign_account_translation() {
	let (mut rc, mut ah) = load_externalities().await.unwrap();

	let mut rc_to_ah = BTreeMap::new();

	rc.execute_with(|| {
		for para_id in paras_registrar::Paras::<Polkadot>::iter_keys().collect::<Vec<_>>() {
			let rc_acc = xcm_builder::ChildParachainConvertsVia::<ParaId, AccountId32>::convert_location(&Location::new(0, Junction::Parachain(para_id.into()))).unwrap();

			let (ah_acc, para_id) = pallet_rc_migrator::accounts::AccountsMigrator::<Polkadot>::try_translate_rc_sovereign_to_ah(rc_acc.clone()).unwrap().unwrap();
			rc_to_ah.insert(rc_acc, (ah_acc, para_id));
		}

		for account in frame_system::Account::<Polkadot>::iter_keys() {
			let translated = pallet_rc_migrator::accounts::AccountsMigrator::<Polkadot>::try_translate_rc_sovereign_to_ah(account.clone()).unwrap();

			if let Some((ah_acc, para_id)) = translated {
				if !rc_to_ah.contains_key(&account) {
					println!("Account belongs to an unregistered para {}: {}", para_id, account);
					rc_to_ah.insert(account, (ah_acc, para_id));
				}
			}
		}
	});

	let mut csv: String = "para,rc,ah\n".into();

	// Sanity check that they all exist. Note that they dont *have to*, but all do.
	println!("Translating {} RC accounts to AH", rc_to_ah.len());
	ah.execute_with(|| {
		for (rc_acc, (ah_acc, para_id)) in rc_to_ah.iter() {
			println!("[{}] {} -> {}", para_id, rc_acc, ah_acc);

			csv.push_str(&format!("{},{},{}\n", para_id, rc_acc, ah_acc));
		}
	});

	//std::fs::write("../../pallets/rc-migrator/src/sovereign_account_translation.csv",
	// csv).unwrap();
}

#[tokio::test]
async fn print_accounts_statistics() {
	use frame_system::Account as SystemAccount;

	let mut rc = remote_ext_test_setup::<PolkadotBlock>("SNAP_RC").await.unwrap();

	let mut total_counts = std::collections::HashMap::new();

	rc.execute_with(|| {
		for (who, account_info) in SystemAccount::<Polkadot>::iter() {
			total_counts.entry("total_count").and_modify(|count| *count += 1).or_insert(1);

			let freezes_count = pallet_balances::Freezes::<Polkadot>::get(&who).len();
			let lock_count = pallet_balances::Locks::<Polkadot>::get(&who).len();
			let holds_sum = pallet_balances::Holds::<Polkadot>::get(&who)
				.iter()
				.map(|h| h.amount)
				.sum::<u128>();
			let unnamed_reserve = account_info.data.reserved.saturating_sub(holds_sum);

			if freezes_count == 0 && lock_count == 0 && holds_sum == 0 && unnamed_reserve == 0 {
				total_counts
					.entry("total_liquid_count")
					.and_modify(|count| *count += 1)
					.or_insert(1);
			}
		}
	});

	/*
	RC Polkadot snapshot from 2025-01-24:
		total_count ~ 1_434_995
		total_liquid_count ~ 1_373_890
	 */
	println!("Total counts: {:?}", total_counts);
}

#[test]
fn ah_account_migration_weight() {
	use frame_support::weights::constants::WEIGHT_REF_TIME_PER_MILLIS;
	use pallet_rc_migrator::weights_ah::WeightInfo;

	let ms_for_accs = |num_accs: u32| {
		let weight =
			pallet_rc_migrator::weights_ah::SubstrateWeight::<AssetHub>::receive_liquid_accounts(
				num_accs as u32,
			);
		weight.ref_time() as f64 / WEIGHT_REF_TIME_PER_MILLIS as f64
	};
	let mb_for_accs = |num_accs: u32| {
		let weight =
			pallet_rc_migrator::weights_ah::SubstrateWeight::<AssetHub>::receive_liquid_accounts(
				num_accs as u32,
			);
		weight.proof_size() as f64 / 1_000_000.0
	};

	// Print for 10, 100 and 1000 accounts in ms
	for i in [10, 100, 486, 1000] {
		let (ms, mb) = (ms_for_accs(i), mb_for_accs(i));
		println!("Weight for {} accounts: {: >4.2} ms, {: >4.2} MB", i, ms, mb);

		assert!(ms < 200.0, "Ref time weight for Accounts migration is insane");
		assert!(mb < 4.0, "Proof size for Accounts migration is insane");
	}
}

#[ignore] // Slow
#[tokio::test(flavor = "current_thread")]
async fn migration_works_time() {
	let Some((mut rc, mut ah)) = load_externalities().await else { return };

	// Set the initial migration stage from env var if set.
	set_initial_migration_stage(&mut rc);

	// Pre-checks on the Relay
	let rc_pre = run_check(|| RcChecks::pre_check(), &mut rc);

	// Pre-checks on the Asset Hub
	let ah_pre = run_check(|| AhChecks::pre_check(rc_pre.clone().unwrap()), &mut ah);

	let rc_block_start = rc.execute_with(|| frame_system::Pallet::<Polkadot>::block_number());
	let ah_block_start = ah.execute_with(|| frame_system::Pallet::<AssetHub>::block_number());

	// we push first message to be popped for the first RC block and the second one to delay the ump
	// messages from the first AH block, since with async backing and full blocks we generally
	// expect the AH+0 block to be backed at RC+2 block, where RC+0 is its parent RC block. Hence
	// the only RC+2 block will receive and process the messages from the AH+0 block.
	let mut ump_messages: VecDeque<(Vec<UpwardMessage>, BlockNumberFor<AssetHub>)> =
		vec![(vec![], ah_block_start - 1), (vec![], ah_block_start)].into();
	// AH generally builds the blocks on every new RC block, therefore every DMP message received
	// and processed immediately without delay.
	let mut dmp_messages: VecDeque<(Vec<InboundDownwardMessage>, BlockNumberFor<Polkadot>)> =
		vec![].into();

	// finish the loop when the migration is done.
	while ah.execute_with(|| AhMigrationStageStorage::<AssetHub>::get()) !=
		AhMigrationStage::MigrationDone
	{
		// with async backing having three unincluded segments, we expect the Asset Hub block
		// to typically be backed not in the immediate next block, but in the block after that.
		// therefore, the queue should always contain at least two messages: one from the most
		// recent Asset Hub block and one from the previous block.
		assert!(ump_messages.len() > 1, "ump_messages queue should contain at least two messages");

		// enqueue UMP messages from AH to RC.
		rc.execute_with(|| {
			enqueue_ump(
				ump_messages.pop_front().expect("should contain at least empty message package"),
			);
		});

		// execute next RC block.
		rc.execute_with(|| {
			next_block_rc();
		});

		// read dmp messages sent to AH.
		dmp_messages.push_back(rc.execute_with(|| {
			(
				DownwardMessageQueues::<Polkadot>::take(AH_PARA_ID),
				frame_system::Pallet::<Polkadot>::block_number(),
			)
		}));

		// end of RC cycle.
		rc.commit_all().unwrap();

		// enqueue DMP messages from RC to AH.
		ah.execute_with(|| {
			enqueue_dmp(
				dmp_messages.pop_front().expect("should contain at least empty message package"),
			);
		});

		// execute next AH block.
		ah.execute_with(|| {
			next_block_ah();
		});

		// collect UMP messages from AH generated by the current block execution.
		ump_messages.push_back(ah.execute_with(|| {
			(
				PendingUpwardMessages::<AssetHub>::take(),
				frame_system::Pallet::<AssetHub>::block_number(),
			)
		}));

		// end of AH cycle.
		ah.commit_all().unwrap();
	}

	let rc_block_end = rc.execute_with(|| frame_system::Pallet::<Polkadot>::block_number());
	let ah_block_end = ah.execute_with(|| frame_system::Pallet::<AssetHub>::block_number());

	// Post-checks on the Relay
	run_check(|| RcChecks::post_check(rc_pre.clone().unwrap()), &mut rc);

	// Post-checks on the Asset Hub
	run_check(|| AhChecks::post_check(rc_pre.unwrap(), ah_pre.unwrap()), &mut ah);

	println!(
		"Migration done in {} RC blocks, {} AH blocks",
		rc_block_end - rc_block_start,
		ah_block_end - ah_block_start
	);
}

#[tokio::test(flavor = "current_thread")]
async fn scheduled_migration_works() {
	let Some((mut rc, mut ah)) = load_externalities().await else { return };

	// Check that the migration is pending on the RC.
	rc.execute_with(|| {
		log::info!("Asserting the initial state on RC");
		next_block_rc();

		assert_eq!(RcMigrationStageStorage::<Polkadot>::get(), RcMigrationStage::Pending);

		// clear the DMP queue.
		let _ = DownwardMessageQueues::<Polkadot>::take(AH_PARA_ID);
	});
	rc.commit_all().unwrap();

	// Check that the migration is pending on the AH.
	ah.execute_with(|| {
		log::info!("Asserting the initial state on AH");
		next_block_ah();

		assert_eq!(AhMigrationStageStorage::<AssetHub>::get(), AhMigrationStage::Pending);

		// clear the UMP queue.
		let _ = PendingUpwardMessages::<AssetHub>::take();
	});
	ah.commit_all().unwrap();

	// Schedule the migration on RC.
	let dmp_messages = rc.execute_with(|| {
		log::info!("Scheduling the migration on RC");
		next_block_rc();

		let now = frame_system::Pallet::<Polkadot>::block_number();
		let scheduled_at = now + 2;

		// Fellowship Origin
		let origin = pallet_xcm::Origin::Xcm(Location::new(
			0,
			[
				Junction::Parachain(1001),
				Junction::Plurality { id: BodyId::Technical, part: BodyPart::Voice },
			],
		));
		assert_ok!(RcMigrator::schedule_migration(origin.into(), DispatchTime::At(scheduled_at)));
		assert_eq!(
			RcMigrationStageStorage::<Polkadot>::get(),
			RcMigrationStage::Scheduled { block_number: scheduled_at }
		);

		next_block_rc();
		// migrating not yet started
		assert_eq!(
			RcMigrationStageStorage::<Polkadot>::get(),
			RcMigrationStage::Scheduled { block_number: scheduled_at }
		);
		assert_eq!(DownwardMessageQueues::<Polkadot>::take(AH_PARA_ID).len(), 0);

		next_block_rc();

		// migration started
		assert_eq!(RcMigrationStageStorage::<Polkadot>::get(), RcMigrationStage::WaitingForAh);
		let dmp_messages = DownwardMessageQueues::<Polkadot>::take(AH_PARA_ID);
		assert!(dmp_messages.len() > 0);

		dmp_messages
	});

	// enqueue DMP messages from RC to AH.
	ah.execute_with(|| {
		enqueue_dmp((dmp_messages, 0u32.into()));
	});
	ah.commit_all().unwrap();

	// Asset Hub receives the message from the Relay Chain to start the migration and the
	// acknowledges it by sending the message back to the Relay Chain.
	let ump_messages = ah.execute_with(|| {
		log::info!("Acknowledging the start of the migration on AH");
		assert_eq!(AhMigrationStageStorage::<AssetHub>::get(), AhMigrationStage::Pending);

		next_block_ah();

		assert_eq!(
			AhMigrationStageStorage::<AssetHub>::get(),
			AhMigrationStage::DataMigrationOngoing
		);

		PendingUpwardMessages::<AssetHub>::take()
	});
	ah.commit_all().unwrap();

	// enqueue UMP messages from AH to RC.
	rc.execute_with(|| {
		enqueue_ump((ump_messages, 0u32.into()));
	});
	rc.commit_all().unwrap();

	// Relay Chain receives the acknowledgement from the Asset Hub and starts sending the data.
	rc.execute_with(|| {
		log::info!("Receiving the acknowledgement from AH on RC");
		assert_eq!(RcMigrationStageStorage::<Polkadot>::get(), RcMigrationStage::WaitingForAh);

		next_block_rc();

		assert_eq!(
			RcMigrationStageStorage::<Polkadot>::get(),
			RcMigrationStage::AccountsMigrationInit
		);
	});
	rc.commit_all().unwrap();
}

#[tokio::test]
async fn some_account_migration_works() {
	use frame_system::Account as SystemAccount;
	use pallet_rc_migrator::accounts::AccountsMigrator;

	let Some((mut rc, mut ah)) = load_externalities().await else { return };

	let accounts: Vec<AccountId32> = vec![
		// 18.03.2025 - account with reserve above ED, but no free balance
		"5HB5nWBF2JfqogQYTcVkP1BfrgfadBizGmLBhmoAbGm5C7ir".parse().unwrap(),
		// 18.03.2025 - account with zero free balance, and reserve below ED
		"5GTtcseuBoAVLbxQ32XRnqkBmxxDaHqdpPs8ktUnH1zE4Cg3".parse().unwrap(),
		// 18.03.2025 - account with free balance below ED, and reserve above ED
		"5HMehBKuxRq7AqdxwQcaM7ff5e8Snchse9cNNGT9wsr4CqBK".parse().unwrap(),
	];

	for account_id in accounts {
		let maybe_withdrawn_account = rc.execute_with(|| {
			let rc_account = SystemAccount::<Polkadot>::get(&account_id);
			log::info!("Migrating account id: {:?}", account_id.to_ss58check());
			log::info!("RC account info: {:?}", rc_account);

			let maybe_withdrawn_account = AccountsMigrator::<Polkadot>::withdraw_account(
				account_id,
				rc_account,
				&mut WeightMeter::new(),
				0,
			)
			.unwrap_or_else(|err| {
				log::error!("Account withdrawal failed: {:?}", err);
				None
			});

			maybe_withdrawn_account
		});

		let withdrawn_account = match maybe_withdrawn_account {
			Some(withdrawn_account) => withdrawn_account,
			None => {
				log::warn!("Account is not withdrawable");
				continue;
			},
		};

		log::info!("Withdrawn account: {:?}", withdrawn_account);

		ah.execute_with(|| {
			use asset_hub_polkadot_runtime::AhMigrator;
			use codec::{Decode, Encode};

			let encoded_account = withdrawn_account.encode();
			let account = Decode::decode(&mut &encoded_account[..]).unwrap();
			let res = AhMigrator::do_receive_account(account);
			log::info!("Account integration result: {:?}", res);
		});
	}
}

#[test]
fn test_account_references() {
	type PalletBalances = pallet_balances::Pallet<Polkadot>;
	type PalletSystem = frame_system::Pallet<Polkadot>;

	new_test_rc_ext().execute_with(|| {
		// create new account.
		let who: AccountId32 = [0; 32].into();
		let ed = <PalletBalances as Currency<_>>::minimum_balance();
		let _ = PalletBalances::deposit_creating(&who, ed + ed + ed);

		// account is create with right balance and references.
		assert_eq!(PalletBalances::balance(&who), ed + ed + ed);
		assert_eq!(PalletSystem::consumers(&who), 0);
		assert_eq!(PalletSystem::providers(&who), 1);

		// decrement consumer reference from `0`.
		PalletSystem::dec_consumers(&who);

		// account is still alive.
		assert_eq!(PalletBalances::balance(&who), ed + ed + ed);
		assert_eq!(PalletSystem::consumers(&who), 0);
		assert_eq!(PalletSystem::providers(&who), 1);

		// reserve some balance which results `+1` consumer reference.
		let _ = PalletBalances::reserve(&who, ed).expect("reserve failed");

		// account data is valid.
		assert_eq!(PalletBalances::balance(&who), ed + ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 1);
		assert_eq!(PalletSystem::providers(&who), 1);

		// force decrement consumer reference from `1`.
		PalletSystem::dec_consumers(&who);

		// account is still alive.
		assert_eq!(PalletBalances::balance(&who), ed + ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 0);
		assert_eq!(PalletSystem::providers(&who), 1);

		// transfer some balance (or perform any update on account) to new account which results
		// consumer reference to automatically correct the consumer reference since the reserve
		// is still there.
		let who2: AccountId32 = [1; 32].into();
		let _ = PalletBalances::transfer(&who, &who2, ed, ExistenceRequirement::AllowDeath)
			.expect("transfer failed");

		// account is still alive, and consumer reference is corrected.
		assert_eq!(PalletBalances::balance(&who), ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 1);
		assert_eq!(PalletSystem::providers(&who), 1);

		// force decrement consumer reference from `1`.
		PalletSystem::dec_consumers(&who);

		// account is still alive, and consumer reference is force decremented.
		assert_eq!(PalletBalances::balance(&who), ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 0);
		assert_eq!(PalletSystem::providers(&who), 1);

		// try to kill the account by transfer all.
		assert_eq!(
			PalletBalances::transfer(&who, &who2, ed + ed, ExistenceRequirement::AllowDeath),
			Err(TokenError::FundsUnavailable.into())
		);

		// account is still alive.
		assert_eq!(PalletBalances::balance(&who), ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 0);
		assert_eq!(PalletSystem::providers(&who), 1);

		// try to transfer all free balance, leaving only reserve.
		assert_eq!(
			PalletBalances::transfer(&who, &who2, ed, ExistenceRequirement::AllowDeath),
			Err(DispatchError::ConsumerRemaining)
		);

		// account is still alive. in this case consumer reference even gets corrected.
		assert_eq!(PalletBalances::balance(&who), ed);
		assert_eq!(PalletBalances::reserved_balance(&who), ed);
		assert_eq!(PalletSystem::consumers(&who), 1);
		assert_eq!(PalletSystem::providers(&who), 1);
	});
}
