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

//! Storage migrations for the collective pallet.
//!
//! This module contains the migration from storage version 4 to version 5,
//! which moves proposal data from inline storage to the preimage pallet.

use super::super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	traits::StorePreimage,
	weights::WeightMeter,
};
use scale_info::TypeInfo;

/// The old storage type for proposals before migration to preimages.
#[frame_support::storage_alias]
pub type OldProposalOf<T: Config<I>, I: 'static> = StorageMap<
	Pallet<T, I>,
	Identity,
	<T as frame_system::Config>::Hash,
	<T as frame_system::Config>::RuntimeCall,
>;

/// The assumed maximum size of a proposal for the first iteration.
///
/// This is used to estimate proof size weight before we know the actual proposal size.
/// 200 KiB should be more than enough for any reasonable proposal.
const ASSUMED_MAX_PROPOSAL_SIZE: u32 = 200 * 1024;

/// Cursor for tracking migration progress across multiple blocks.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum Cursor {
	/// Migrating proposals at the given index, with the maximum observed proposal size.
	MigrateProposals { index: u32, max_proposal_len: u32 },
	/// Clearing old storage entries.
	ClearStorage,
}

/// Migration to move proposal storage to the preimage pallet.
///
/// This migration reads proposals from the old `ProposalOf` storage map and
/// stores them using the preimage pallet, then clears the old storage.
pub struct MigrateToV5<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> SteppedMigration for MigrateToV5<T, I> {
	type Cursor = Cursor;
	type Identifier = [u8; 32];

	fn id() -> Self::Identifier {
		*b"CollectiveMigrationV5___________"
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let cursor = cursor.unwrap_or(Cursor::MigrateProposals {
			index: 0,
			max_proposal_len: ASSUMED_MAX_PROPOSAL_SIZE,
		});

		match cursor {
			Cursor::MigrateProposals { index, max_proposal_len } => {
				let proposals = Proposals::<T, I>::get();
				let count = proposals.len() as u32;

				let mut current_index = index;
				let mut current_max_len = max_proposal_len;

				while current_index < count {
					// Conservative weight estimate per proposal:
					// - OldProposalOf::get: 1 read
					// - Preimages::note internally does:
					//   - StatusFor::take (migration check): 1 read
					//   - RequestStatusFor::get: 1 read
					//   - RequestStatusFor::insert: 1 write
					//   - PreimageFor::insert: 1 write
					// - OldProposalOf::remove: 1 write
					// Total: 3 reads, 3 writes (minimum)
					let weight_per_item = T::DbWeight::get()
						.reads_writes(4, 4)
						.saturating_add(Weight::from_parts(0, current_max_len as u64));

					if meter.try_consume(weight_per_item).is_err() {
						return Ok(Some(Cursor::MigrateProposals {
							index: current_index,
							max_proposal_len: current_max_len,
						}));
					}

					let hash = proposals[current_index as usize];
					if let Some(proposal) = OldProposalOf::<T, I>::get(hash) {
						let encoded = proposal.encode();
						// Track the maximum proposal length for future weight estimation.
						current_max_len = current_max_len.max(encoded.len() as u32);
						let _ = T::Preimages::note(encoded.into());
						OldProposalOf::<T, I>::remove(hash);
					}

					current_index += 1;
				}

				Ok(Some(Cursor::ClearStorage))
			},
			Cursor::ClearStorage => {
				let limit = 100u32;
				let result = OldProposalOf::<T, I>::clear(limit, None);

				let weight =
					T::DbWeight::get().reads_writes(1, 1).saturating_mul(result.loops as u64);
				meter.consume(weight);

				if result.maybe_cursor.is_some() {
					Ok(Some(Cursor::ClearStorage))
				} else {
					Ok(None)
				}
			},
		}
	}
}
