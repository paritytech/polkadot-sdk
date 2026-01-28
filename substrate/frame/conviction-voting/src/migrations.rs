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

use super::*;
use frame_support::{pallet_prelude::*, traits::OnRuntimeUpgrade};

pub mod v1 {
	use super::*;

	/// Migration to remove empty VotingFor and ClassLocksFor entries
	pub struct CleanupEmptyStorage<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for CleanupEmptyStorage<T, I> {
		fn on_runtime_upgrade() -> Weight {
			let mut reads = 0u64;
			let mut writes = 0u64;

			log::info!("🔧 Starting conviction-voting storage cleanup migration");

			// Clean up VotingFor entries
			let mut voting_removed = 0u64;
			for (account, class, voting) in VotingFor::<T, I>::iter() {
				reads += 1;
				if Pallet::<T, I>::is_empty_voting(&voting) {
					VotingFor::<T, I>::remove(&account, &class);
					writes += 1;
					voting_removed += 1;
				}
			}

			// Clean up ClassLocksFor entries
			let mut locks_removed = 0u64;
			for (account, mut locks) in ClassLocksFor::<T, I>::iter() {
				reads += 1;
				let original_len = locks.len();
				locks.retain(|(_, balance)| !balance.is_zero());

				if locks.is_empty() {
					ClassLocksFor::<T, I>::remove(&account);
					writes += 1;
					locks_removed += 1;
				} else if locks.len() != original_len {
					ClassLocksFor::<T, I>::insert(&account, locks);
					writes += 1;
				}
			}

			log::info!(
                "✅ Conviction-voting cleanup complete: removed {} empty VotingFor entries and {} empty ClassLocksFor entries",
                voting_removed,
                locks_removed
            );

			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let voting_count = VotingFor::<T, I>::iter()
				.filter(|(_, _, voting)| Pallet::<T, I>::is_empty_voting(voting))
				.count();

			let locks_count = ClassLocksFor::<T, I>::iter()
				.filter(|(_, locks)| locks.iter().all(|(_, balance)| balance.is_zero()))
				.count();

			log::info!(
                "🔍 Pre-upgrade: found {} empty VotingFor entries and {} empty ClassLocksFor entries",
                voting_count,
                locks_count
            );

			Ok((voting_count as u64, locks_count as u64).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (expected_voting, expected_locks): (u64, u64) =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides valid data; qed");

			let remaining_empty_voting = VotingFor::<T, I>::iter()
				.filter(|(_, _, voting)| Pallet::<T, I>::is_empty_voting(voting))
				.count();

			let remaining_empty_locks = ClassLocksFor::<T, I>::iter()
				.filter(|(_, locks)| locks.iter().all(|(_, balance)| balance.is_zero()))
				.count();

			assert_eq!(remaining_empty_voting, 0, "All empty VotingFor entries should be removed");
			assert_eq!(
				remaining_empty_locks, 0,
				"All empty ClassLocksFor entries should be removed"
			);

			log::info!(
				"✅ Post-upgrade: successfully removed {} VotingFor and {} ClassLocksFor entries",
				expected_voting,
				expected_locks
			);

			Ok(())
		}
	}
}
