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

use crate::{mock::*, people, InitializeIndividualityPallets};
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};

#[test]
fn successful_individuality_initialisation() {
	new_test_ext().execute_with(|| {
		// Make sure that the chunks are empty
		assert_eq!(people::Chunks::<Test>::iter().count(), 0);

		// Make sure there are no people, nor keys and that the onboarding queue is empty
		assert_eq!(pallet_people::People::<Test>::iter().count(), 0);
		assert_eq!(pallet_people::Keys::<Test>::iter().count(), 0);

		let (head, _) = pallet_people::QueuePageIndices::<Test>::get();
		assert_eq!(pallet_people::OnboardingQueue::<Test>::get(head).len(), 0);

		// Start the migration
		let mut weight_meter = WeightMeter::new();
		let mut cursor = None;
		while let Some(new_cursor) =
			InitializeIndividualityPallets::<Test>::step(cursor, &mut weight_meter).unwrap()
		{
			cursor = Some(new_cursor);
		}

		// Check the chunks - should be filled now
		assert_ne!(people::Chunks::<Test>::iter().count(), 0);

		// Check if the initial set of recognised people was added to the people pallet
		assert_ne!(pallet_people::People::<Test>::iter().count(), 0);
		assert_ne!(pallet_people::Keys::<Test>::iter().count(), 0);

		let (head, _) = pallet_people::QueuePageIndices::<Test>::get();
		assert_ne!(pallet_people::OnboardingQueue::<Test>::get(head).len(), 0);
	});
}
