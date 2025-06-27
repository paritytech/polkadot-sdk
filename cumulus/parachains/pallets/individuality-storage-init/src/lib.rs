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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	weights::WeightMeter,
};
use pallet_people::MemberOf;
use sp_core::Get;
use sp_std::{vec, vec::Vec};
use verifiable::ring_vrf_impl::{BandersnatchVrfVerifiable, StaticChunk};

#[cfg(feature = "try-runtime")]
use alloc::collections::BTreeMap;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub use pallet::*;

pub trait WeightInfo {}

impl WeightInfo for () {}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_people::Config {
		/// Overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for the extrinsics in the pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	pub enum Event<T: Config> {}
}

pub const PALLET_ID: &[u8; 25] = b"pallet-individuality-init";

mod people {
	use super::*;

	#[storage_alias]
	pub type Chunks<T: Config> = StorageMap<
		People,
		Twox64Concat,
		u32,
		BoundedVec<StaticChunk, <T as pallet_people::Config>::ChunkPageSize>,
		OptionQuery,
	>;
}

mod identity {
	// use super::*;

	// Registrars

	// AuthorityOf
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen)]
pub enum MigrationState {
	NotStarted,
	InitializingChunksForPalletPeople { last_page: u32 },
	InitializingPeopleForPalletPeople,
	// InitializingPalletIdentity,
	Finished,
}

pub struct InitializeIndividualityPallets<T>(PhantomData<T>);

impl<T: Config> SteppedMigration for InitializeIndividualityPallets<T> {
	type Cursor = MigrationState;
	type Identifier = MigrationId<25>;

	fn id() -> <InitializeIndividualityPallets<T> as SteppedMigration>::Identifier {
		MigrationId { pallet_id: *PALLET_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<<InitializeIndividualityPallets<T> as SteppedMigration>::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<
		Option<<InitializeIndividualityPallets<T> as SteppedMigration>::Cursor>,
		SteppedMigrationError,
	> {
		let state = cursor.unwrap_or(MigrationState::NotStarted);

		match state {
			MigrationState::NotStarted => {
				log::info!("Individuality pallets initialization is about to start");

				Ok(Some(MigrationState::InitializingChunksForPalletPeople { last_page: 0 }))
			},

			MigrationState::InitializingChunksForPalletPeople { last_page } => {
				log::info!(
					"Individuality pallets initialization - adding chunks for pallet people"
				);

				let chunks = get_chunks();
				assert_eq!(chunks.len(), 1 << 9);

				let chunk_page_size = T::ChunkPageSize::get() as usize;
				let total_pages = (chunks.len() + chunk_page_size - 1) / chunk_page_size;

				const PAGES_PER_STEP: u32 = 10;
				let start_page = last_page as usize;
				let end_page = ((last_page + PAGES_PER_STEP) as usize).min(total_pages);

				for page_idx in start_page..end_page {
					let weight = T::DbWeight::get().writes(1);
					if !meter.can_consume(weight) {
						return Ok(Some(MigrationState::InitializingChunksForPalletPeople {
							last_page: page_idx as u32,
						}));
					}

					let start_idx = page_idx * chunk_page_size;
					let end_idx = ((page_idx + 1) * chunk_page_size).min(chunks.len());

					let page_chunks: BoundedVec<_, _> = chunks[start_idx..end_idx]
						.to_vec()
						.try_into()
						.map_err(|_| SteppedMigrationError::Failed)?;

					people::Chunks::<T>::insert(page_idx as u32, page_chunks);
					meter.consume(weight);
				}

				if end_page >= total_pages {
					Ok(Some(MigrationState::InitializingPeopleForPalletPeople))
				} else {
					Ok(Some(MigrationState::InitializingChunksForPalletPeople {
						last_page: end_page as u32,
					}))
				}
			},

			MigrationState::InitializingPeopleForPalletPeople => {
				log::info!("Individuality pallets initialization - adding people to pallet people");

				let initial_people = get_initial_people_keys();
				let keys_count = initial_people.len();

				// TODO replace with benchmarked weight
				let weight =
					T::DbWeight::get().writes(1) + T::DbWeight::get().writes(keys_count as u64 * 2);

				if !meter.can_consume(weight) {
					return Ok(Some(MigrationState::InitializingPeopleForPalletPeople));
				}

				let keys: Vec<MemberOf<T>> = initial_people
					.into_iter()
					.map(|raw_key| {
						use codec::Decode;
						MemberOf::<T>::decode(&mut &raw_key[..])
							.map_err(|_| SteppedMigrationError::Failed)
					})
					.collect::<Result<Vec<_>, _>>()?;

				pallet_people::Pallet::<T>::force_recognize_personhood(
					frame_system::RawOrigin::Root.into(),
					keys,
				)
				.map_err(|_| SteppedMigrationError::Failed)?;

				meter.consume(weight);
				Ok(Some(MigrationState::Finished))
			},

			MigrationState::Finished => {
				log::info!("Individuality pallets initialization completed");
				Ok(None)
			},
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		ensure!(
			people::Chunks::<T>::iter().count() == 0,
			"Chunks storage should be empty before migration"
		);

		ensure!(
			pallet_people::People::<T>::iter().count() == 0,
			"People storage should be empty before migration"
		);

		ensure!(
			pallet_people::NextPersonalId::<T>::get() == 0,
			"NextPersonalId should be 0 before migration"
		);

		let chunks = get_chunks();
		let people = get_initial_people_keys();
		let state = (chunks.len() as u32, people.len() as u32);

		Ok(state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		let (expected_chunks_count, expected_people_count): (u32, u32) =
			Decode::decode(&mut &state[..]).map_err(|_| "Failed to decode state")?;

		// To verify chunks were loaded correctly
		let actual_chunks_count: u32 =
			people::Chunks::<T>::iter().map(|(_, page)| page.len() as u32).sum();
		ensure!(actual_chunks_count == expected_chunks_count, "Chunks count mismatch");

		// To verify people were loaded correctly
		let actual_people_count = pallet_people::People::<T>::iter().count() as u32;
		ensure!(actual_people_count == expected_people_count, "People count mismatch");

		// To check keys were populated
		let keys_count = pallet_people::Keys::<T>::iter().count() as u32;
		ensure!(keys_count == expected_people_count, "Keys count mismatch");

		// To verify all people were added to the OnboardingQueue
		let (head, tail) = pallet_people::QueuePageIndices::<T>::get();
		let total_in_queue = pallet_people::OnboardingQueue::get(head).len() as u32;
		ensure!(total_in_queue == expected_people_count, "OnboardingQueue count mismatch");

		// To verify NextPersonalId is set correctly if people were added
		if expected_people_count > 0 {
			let next_id = pallet_people::NextPersonalId::<T>::get();
			ensure!(
				next_id >= expected_people_count as PersonalId,
				"NextPersonalId not set correctly"
			);
		}

		Ok(())
	}
}

fn get_chunks() -> Vec<<BandersnatchVrfVerifiable as verifiable::GenerateVerifiable>::StaticChunk> {
	let params = verifiable::ring_vrf_impl::ring_verifier_builder_params();
	let chunks: Vec<StaticChunk> = params.0.iter().map(|c| StaticChunk(*c)).collect();
	chunks
}

/// Keys of the initial set of people to be recognized during migration.
/// These are encoded Bandersnatch public keys.
fn get_initial_people_keys() -> Vec<Vec<u8>> {
	use hex_literal::hex;

	vec![
		hex!("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d").to_vec(),
		hex!("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48").to_vec(),
		hex!("90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22").to_vec(),
	]
}
