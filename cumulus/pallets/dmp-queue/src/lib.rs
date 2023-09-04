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

//! This pallet is a state machine to migrate the remaining DMP messages into to a generic
//! `HandleMessage`. It proceeds in the state of [`MigrationState`] one by one by their listing in
//! the source code. The pallet can be removed from the runtime once `Completed` was emitted.

#![cfg_attr(not(feature = "std"), no_std)]

use migration::*;
pub use pallet::*;

mod migration;
mod mock;
mod tests;

pub(crate) const LOG: &str = "dmp-queue-export-xcms";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, traits::HandleMessage};
	use frame_system::pallet_prelude::*;
	use sp_io::hashing::twox_128;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type of the runtime.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type DmpSink: HandleMessage;
	}

	#[pallet::storage]
	pub type MigrationStatus<T> = StorageValue<_, MigrationState, ValueQuery>;

	#[derive(
		codec::Encode, codec::Decode, Debug, PartialEq, Eq, Clone, MaxEncodedLen, TypeInfo,
	)]
	pub enum MigrationState {
		NotStarted,
		StartedExport {
			/// The next page that should be exported.
			next_begin_used: PageCounter,
		},
		CompletedExport,
		StartedOverweightExport {
			/// The next overweight index that should be exported.
			next_overweight_index: u64,
		},
		CompletedOverweightExport,
		StartedCleanup {
			cursor: Option<BoundedVec<u8, ConstU32<1024>>>,
		},
		Completed,
	}

	impl Default for MigrationState {
		fn default() -> Self {
			Self::NotStarted
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		StartedExport,
		Exported { page: PageCounter },
		CompletedExport,
		StartedOverweightExport,
		ExportedOverweight { index: OverweightIndex },
		CompletedOverweightExport,
		StartedCleanup,
		CleanedSome { keys_removed: u32 },
		Completed { error: bool },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			let state = MigrationStatus::<T>::get();
			let index = PageIndex::<T>::get();
			log::info!(target: LOG, "on_initialize: block={:?}, state={:?}, index={:?}", now, state, index);

			match state {
				MigrationState::NotStarted => {
					log::info!(target: LOG, "Init export at page {}", index.begin_used);

					MigrationStatus::<T>::put(MigrationState::StartedExport {
						next_begin_used: index.begin_used,
					});
					Self::deposit_event(Event::StartedExport);
				},
				MigrationState::StartedExport { next_begin_used } => {
					log::info!(target: LOG, "Exporting page {}", next_begin_used);

					if next_begin_used == index.end_used {
						MigrationStatus::<T>::put(MigrationState::CompletedExport);
						Self::deposit_event(Event::CompletedExport);
						log::info!(target: LOG, "CompletedExport");
					} else {
						migration::migrate_page::<T>(next_begin_used);

						MigrationStatus::<T>::put(MigrationState::StartedExport {
							next_begin_used: next_begin_used + 1,
						});
						Self::deposit_event(Event::Exported { page: next_begin_used });
						log::info!(target: LOG, "Exported page {}", next_begin_used);
					}
				},
				MigrationState::CompletedExport => {
					log::info!(target: LOG, "Init export overweight at index 0");

					MigrationStatus::<T>::put(MigrationState::StartedOverweightExport {
						next_overweight_index: 0,
					});
					Self::deposit_event(Event::StartedOverweightExport);
				},
				MigrationState::StartedOverweightExport { next_overweight_index } => {
					log::info!(target: LOG, "Exporting overweight index {}", next_overweight_index);

					if next_overweight_index == index.overweight_count {
						MigrationStatus::<T>::put(MigrationState::CompletedOverweightExport);
						Self::deposit_event(Event::CompletedOverweightExport);
						log::info!(target: LOG, "CompletedOverweightExport");
					} else {
						migration::migrate_overweight::<T>(next_overweight_index);

						MigrationStatus::<T>::put(MigrationState::StartedOverweightExport {
							next_overweight_index: next_overweight_index + 1,
						});
						Self::deposit_event(Event::ExportedOverweight {
							index: next_overweight_index,
						});
						log::info!(target: LOG, "Exported overweight index {}", next_overweight_index);
					}
				},
				MigrationState::CompletedOverweightExport => {
					log::info!(target: LOG, "Init cleanup");

					MigrationStatus::<T>::put(MigrationState::StartedCleanup { cursor: None });
					Self::deposit_event(Event::StartedCleanup);
				},
				MigrationState::StartedCleanup { cursor } => {
					log::info!(target: LOG, "Cleaning up");
					let hashed_prefix =
						twox_128(<Pallet<T> as PalletInfoAccess>::name().as_bytes());

					let result = frame_support::storage::unhashed::clear_prefix(
						&hashed_prefix,
						Some(2), // Somehow it does nothing when set to 1, so we set it to 2.
						cursor.as_ref().map(|c| c.as_ref()),
					);
					Self::deposit_event(Event::CleanedSome { keys_removed: result.backend });
					// GOTCHA: We delete *all* pallet storage; hence we also delete our own
					// `MigrationState`. BUT we insert it back into storage:

					if let Some(unbound_cursor) = result.maybe_cursor {
						if let Ok(cursor) = unbound_cursor.try_into() {
							log::info!(target: LOG, "Next cursor: {:?}", &cursor);
							MigrationStatus::<T>::put(MigrationState::StartedCleanup {
								cursor: Some(cursor),
							});
						} else {
							MigrationStatus::<T>::put(MigrationState::Completed);
							Self::deposit_event(Event::Completed { error: true });
							log::info!(target: LOG, "Completed with error: could not bound cursor");
						}
					} else {
						MigrationStatus::<T>::put(MigrationState::Completed);
						Self::deposit_event(Event::Completed { error: false });
						log::info!(target: LOG, "Completed");
					}
				},
				MigrationState::Completed => {
					log::info!(target: LOG, "Idle; you can remove the pallet");
				},
			}

			Weight::MAX // FAIL-CI what do?
		}
	}
}
