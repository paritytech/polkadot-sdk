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

use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	storage_alias,
	weights::WeightMeter,
};
use sp_core::Get;
use sp_std::vec::Vec;
use verifiable::{
	ring_vrf_impl::{BandersnatchVrfVerifiable, StaticChunk},
};

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

#[storage_alias]
type ChunksInPeople<T: Config> = StorageMap<
	pallet_people::Pallet<T>,
	Twox64Concat,
	u32,
	BoundedVec<StaticChunk, <T as pallet_people::Config>::ChunkPageSize>,
	OptionQuery,
>;

#[storage_alias]
type OnboardingSizeInPeople<T: Config> = StorageValue<pallet_people::Pallet<T>, u32, ValueQuery>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen)]
pub enum MigrationState {
	NotStarted,
	InitializingChunks { last_page: u32 },
	InitializingOtherPallet { progress: u32 },
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
		mut cursor: Option<<InitializeIndividualityPallets<T> as SteppedMigration>::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<
		Option<<InitializeIndividualityPallets<T> as SteppedMigration>::Cursor>,
		SteppedMigrationError,
	> {
		let state = cursor.unwrap_or(MigrationState::NotStarted);

		match state {
			MigrationState::NotStarted => {
				// Start with People pallet chunks
				Ok(Some(MigrationState::InitializingChunks { last_page: 0 }))
			},

			MigrationState::InitializingChunks { last_page } => {
				// Initialize chunks for People pallet
				let chunks = get_chunks();
				assert_eq!(chunks.len(), 1 << 9);

				let chunk_page_size = T::ChunkPageSize::get() as usize;
				let total_pages = (chunks.len() + chunk_page_size - 1) / chunk_page_size;

				// Process a few pages per step
				const PAGES_PER_STEP: u32 = 10;
				let start_page = last_page as usize;
				let end_page = ((last_page + PAGES_PER_STEP) as usize).min(total_pages);

				for page_idx in start_page..end_page {
					let weight = T::DbWeight::get().writes(1);
					if !meter.can_consume(weight) {
						return Ok(Some(MigrationState::InitializingChunks {
							last_page: page_idx as u32,
						}));
					}

					let start_idx = page_idx * chunk_page_size;
					let end_idx = ((page_idx + 1) * chunk_page_size).min(chunks.len());

					let page_chunks: BoundedVec<_, _> = chunks[start_idx..end_idx]
						.to_vec()
						.try_into()
						.map_err(|_| SteppedMigrationError::Failed)?;

					ChunksInPeople::<T>::insert(page_idx as u32, page_chunks);
					meter.consume(weight);
				}

				if end_page >= total_pages {
					// Move to next pallet initialization
					Ok(Some(MigrationState::InitializingOtherPallet { progress: 0 }))
				} else {
					Ok(Some(MigrationState::InitializingChunks { last_page: end_page as u32 }))
				}
			},

			MigrationState::InitializingOtherPallet { progress } => {
				// Initialize other pallets' storage
				// Example: Set onboarding size for People pallet
				if progress == 0 {
					let weight = T::DbWeight::get().writes(1);
					if !meter.can_consume(weight) {
						return Ok(Some(state));
					}

					OnboardingSizeInPeople::<T>::put(T::MaxRingSize::get());
					meter.consume(weight);

					return Ok(Some(MigrationState::InitializingOtherPallet { progress: 1 }));
				}

				// Add more initialization steps for other pallets...

				Ok(Some(MigrationState::Finished))
			},

			MigrationState::Finished => {
				log::info!("Individuality pallets initialization completed!");
				Ok(None)
			},
		}
	}
}

fn get_chunks() -> Vec<<BandersnatchVrfVerifiable as verifiable::GenerateVerifiable>::StaticChunk> {
	let params = verifiable::ring_vrf_impl::ring_verifier_builder_params();
	let chunks: Vec<StaticChunk> = params.0.iter().map(|c| StaticChunk(*c)).collect();
	chunks
}
