// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg(test)]
mod mock;

pub(crate) const LOG_TARGET: &str = "integration-tests::epm-staking";

use mock::*;

use frame_election_provider_support::{bounds::ElectionBoundsBuilder, ElectionDataProvider};

use frame_support::{assert_ok, traits::UnixTime};

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("🛠️  ", $patter)  $(, $values)*
		)
	};
}

fn log_current_time() {
	log!(
		info,
		"block: {:?}, session: {:?}, era: {:?}, EPM phase: {:?} ts: {:?}",
		System::block_number(),
		Session::current_index(),
		Staking::current_era(),
		ElectionProvider::current_phase(),
		Timestamp::now()
	);
}

#[test]
fn block_progression_works() {
	let (mut ext, _pool_state, _) = ExtBuilder::default().build_offchainify();
	ext.execute_with(|| {})
}

#[test]
fn verify_snapshot() {
	ExtBuilder::default().build_and_execute(|| {
		assert_eq!(Pages::get(), 3);

		// manually get targets and voters from staking to see the inspect the issue with the
		// DataProvider.
		let bounds = ElectionBoundsBuilder::default()
			.targets_count((TargetSnapshotPerBlock::get() as u32).into())
			.voters_count((VoterSnapshotPerBlock::get() as u32).into())
			.build();

		assert_ok!(<Staking as ElectionDataProvider>::electable_targets(bounds.targets, 2));
		assert_ok!(<Staking as ElectionDataProvider>::electing_voters(bounds.voters, 2));
	})
}

mod staking_integration {
	use super::*;
	use pallet_election_provider_multi_block::Phase;

	#[test]
	fn call_elect_multi_block() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(Pages::get(), 3);
			assert_eq!(ElectionProvider::current_round(), 0);
			assert_eq!(Staking::current_era(), Some(0));

			let export_starts_at = election_prediction() - Pages::get();

			assert!(Staking::election_data_lock().is_none());

			// check that the election data provider lock is set during the snapshot phase and
			// released afterwards.
			roll_to_phase(Phase::Snapshot(Pages::get() - 1), false);
			assert!(Staking::election_data_lock().is_some());

			roll_one(None, false);
			assert!(Staking::election_data_lock().is_some());
			roll_one(None, false);
			assert!(Staking::election_data_lock().is_some());
			// snapshot phase done, election data lock was released.
			roll_one(None, false);
			assert_eq!(ElectionProvider::current_phase(), Phase::Signed);
			assert!(Staking::election_data_lock().is_none());

			// last block where phase is waiting for unsignned submissions.
			roll_to(election_prediction() - 4, false);
			assert_eq!(ElectionProvider::current_phase(), Phase::Unsigned(17));

			// staking prepares first page of exposures.
			roll_to(export_starts_at, false);
			assert_eq!(ElectionProvider::current_phase(), Phase::Export(export_starts_at));

			// staking prepares second page of exposures.
			roll_to(election_prediction() - 2, false);
			assert_eq!(ElectionProvider::current_phase(), Phase::Export(export_starts_at));

			// staking prepares third page of exposures.
			roll_to(election_prediction() - 1, false);

			// election successfully, round & era progressed.
			assert_eq!(ElectionProvider::current_phase(), Phase::Off);
			assert_eq!(ElectionProvider::current_round(), 1);
			assert_eq!(Staking::current_era(), Some(1));
		})
	}
}
