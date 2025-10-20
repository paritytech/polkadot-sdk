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

//! Tests for try-state checks.

use super::*;
use frame_support::assert_ok;

#[test]
fn try_state_works_with_uninitialized_pallet() {
	sp_io::TestExternalities::default().execute_with(|| {
		// Verify the pallet is uninitialized
		assert!(ActiveEra::<Test>::get().is_none());
		assert!(CurrentEra::<Test>::get().is_none());
		assert_eq!(Bonded::<Test>::iter().count(), 0);
		assert_eq!(Ledger::<Test>::iter().count(), 0);
		assert_eq!(Validators::<Test>::iter().count(), 0);
		assert_eq!(Nominators::<Test>::iter().count(), 0);

		// Try-state should pass with uninitialized state
		assert_ok!(Staking::do_try_state(System::block_number()));
	});
}

#[test]
fn try_state_detects_inconsistent_active_current_era() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
		// Set only ActiveEra (CurrentEra remains None) - this violates the invariant
		ActiveEra::<Test>::put(ActiveEraInfo { index: 1, start: None });
		CurrentEra::<Test>::kill();

		// Try-state should fail due to inconsistent state
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// Now set only CurrentEra (ActiveEra None) - this also violates the invariant
		ActiveEra::<Test>::kill();
		CurrentEra::<Test>::put(1);

		// Try-state should fail due to inconsistent state
		assert!(Staking::do_try_state(System::block_number()).is_err());

		// Both None should pass
		ActiveEra::<Test>::kill();
		CurrentEra::<Test>::kill();
		assert_ok!(Staking::do_try_state(System::block_number()));

		// Both Some should pass (assuming other invariants are met)
		ActiveEra::<Test>::put(ActiveEraInfo { index: 1, start: None });
		CurrentEra::<Test>::put(1);
		// Need to set up bonded eras for this to pass
		use frame_support::BoundedVec;
		let bonded_eras: BoundedVec<(u32, u32), _> =
			BoundedVec::try_from(vec![(0, 0), (1, 0)]).unwrap();
		BondedEras::<Test>::put(bonded_eras);
		assert_ok!(Staking::do_try_state(System::block_number()));
	});
}
