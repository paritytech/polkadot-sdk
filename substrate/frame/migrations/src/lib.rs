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

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! # `pallet-migrations`
//!
//! Provides multi block migrations for FRAME runtimes.
//!
//! ## Overview
//!
//! The pallet takes care of executing a batch of multi-step migrations over multiple blocks. The
//! process starts on each runtime upgrade. Normal and operational transactions are paused while
//! migrations are on-going.
//!
//! ### Example
//!
//! This example demonstrates a simple mocked walk through of a basic success scenario. The pallet
//! is configured with two migrations: one succeeding after just one step, and the second one
//! succeeding after two steps. A runtime upgrade is then enacted and the block number is advanced
//! until all migrations finish executing. Afterwards, the recorded historic migrations are
//! checked and events are asserted.
#![doc = docify::embed!("src/tests.rs", simple_works)]
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! Otherwise noteworthy API of this pallet include its implementation of the
//! [`MultiStepMigrator`] trait. This must be plugged into
//! [`frame_system::Config::MultiBlockMigrator`] for proper function.
//!
//! The API contains some calls for emergency management. They are all prefixed with `force_` and
//! should normally not be needed. Pay special attention prior to using them.
//!
//! ### Design Goals
//!
//! 1. Must automatically execute migrations over multiple blocks.
//! 2. Must expose information about whether migrations are ongoing.
//! 3. Must respect pessimistic weight bounds of migrations.
//! 4. Must execute migrations in order. Skipping is not allowed; migrations are run on a
//! all-or-nothing basis.
//! 5. Must prevent re-execution of past migrations.
//! 6. Must provide transactional storage semantics for migrations.
//! 7. Must guarantee progress.
//!
//! ### Design
//!
//! Migrations are provided to the pallet through the associated type [`Config::Migrations`] of type
//! [`SteppedMigrations`]. This allows multiple migrations to be aggregated through a tuple. It
//! simplifies the trait bounds since all associated types of the trait must be provided by the
//! pallet. The actual progress of the pallet is stored in the [`Cursor`] storage item. This can
//! either be [`MigrationCursor::Active`] or [`MigrationCursor::Stuck`]. In the active case it
//! points to the currently active migration and stores its inner cursor. The inner cursor can then
//! be used by the migration to store its inner state and advance. Each time when the migration
//! returns `Some(cursor)`, it signals the pallet that it is not done yet.  
//! The cursor is reset on each runtime upgrade. This ensures that it starts to execute at the
//! first migration in the vector. The pallets cursor is only ever incremented or set to `Stuck`
//! once it encounters an error (Goal 4). Once in the stuck state, the pallet will stay stuck until
//! it is fixed through manual governance intervention.  
//! As soon as the cursor of the pallet becomes `Some(_)`; [`MultiStepMigrator::ongoing`] returns
//! `true` (Goal 2). This can be used by upstream code to possibly pause transactions.
//! In `on_initialize` the pallet will load the current migration and check whether it was already
//! executed in the past by checking for membership of its ID in the [`Historic`] set. Historic
//! migrations are skipped without causing an error. Each successfully executed migration is added
//! to this set (Goal 5).  
//! This proceeds until no more migrations remain. At that point, the event `UpgradeCompleted` is
//! emitted (Goal 1).  
//! The execution of each migration happens by calling [`SteppedMigration::transactional_step`].
//! This function wraps the inner `step` function into a transactional layer to allow rollback in
//! the error case (Goal 6).  
//! Weight limits must be checked by the migration itself. The pallet provides a [`WeightMeter`] for
//! that purpose. The pallet may return [`SteppedMigrationError::InsufficientWeight`] at any point.
//! In that scenario, one of two things will happen: if that migration was exclusively executed
//! in this block, and therefore required more than the maximum amount of weight possible, the
//! process becomes `Stuck`. Otherwise, one re-attempt is executed with the same logic in the next
//! block (Goal 3). Progress through the migrations is guaranteed by providing a timeout for each
//! migration via [`SteppedMigration::max_steps`]. The pallet **ONLY** guarantees progress if this
//! is set to sensible limits (Goal 7).
//!
//! ### Scenario: Governance cleanup
//!
//! Every now and then, governance can make use of the [`clear_historic`][Pallet::clear_historic]
//! call. This ensures that no old migrations pile up in the [`Historic`] set. This can be done very
//! rarely, since the storage should not grow quickly and the lookup weight does not suffer much.
//! Another possibility would be to have a synchronous single-block migration perpetually deployed
//! that cleans them up before the MBMs start.
//!
//! ### Scenario: Successful upgrade
//!
//! The standard procedure for a successful runtime upgrade can look like this:
//! 1. Migrations are configured in the `Migrations` config item. All migrations expose
//! [`max_steps`][SteppedMigration::max_steps], are error tolerant, check their weight bounds and
//! have a unique identifier.
//! 2. The runtime upgrade is enacted. An `UpgradeStarted` event is
//! followed by lots of `MigrationAdvanced` and `MigrationCompleted` events. Finally
//! `UpgradeCompleted` is emitted.
//! 3. Cleanup as described in the governance scenario be executed at any time after the migrations
//! completed.
//!
//! ### Advice: Failed upgrades
//!
//! Failed upgrades cannot be recovered from automatically and require governance intervention. Set
//! up monitoring for `UpgradeFailed` events to be made aware of any failures. The hook
//! [`FailedMigrationHandler::failed`] should be setup in a way that it allows governance to act,
//! but still prevent other transactions from interacting with the inconsistent storage state. Note
//! that this is paramount, since the inconsistent state might contain a faulty balance amount or
//! similar that could cause great harm if user transactions don't remain suspended. One way to
//! implement this would be to use the `SafeMode` or `TxPause` pallets that can prevent most user
//! interactions but still allow a whitelisted set of governance calls.
//!
//! ### Remark: Failed migrations
//!
//! Failed migrations are not added to the `Historic` set. This means that an erroneous
//! migration must be removed and fixed manually. This already applies, even before considering the
//! historic set.
//!
//! ### Remark: Transactional processing
//!
//! You can see the transactional semantics for migration steps as mostly useless, since in the
//! stuck case the state is already messed up. This just prevents it from becoming even more messed
//! up, but doesn't prevent it in the first place.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod mock;
pub mod mock_helpers;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

use codec::{Decode, Encode, MaxEncodedLen};
use core::ops::ControlFlow;
use frame_support::{
	defensive, defensive_assert,
	migrations::*,
	traits::Get,
	weights::{Weight, WeightMeter},
	BoundedVec,
};
use frame_system::{pallet_prelude::BlockNumberFor, Pallet as System};
use sp_runtime::Saturating;
use sp_std::vec::Vec;

/// Points to the next migration to execute.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo, MaxEncodedLen)]
pub enum MigrationCursor<Cursor, BlockNumber> {
	/// Points to the currently active migration and its inner cursor.
	Active(ActiveCursor<Cursor, BlockNumber>),

	/// Migration got stuck and cannot proceed. This is bad.
	Stuck,
}

impl<Cursor, BlockNumber> MigrationCursor<Cursor, BlockNumber> {
	/// Try to return self as an [`ActiveCursor`].
	pub fn as_active(&self) -> Option<&ActiveCursor<Cursor, BlockNumber>> {
		match self {
			MigrationCursor::Active(active) => Some(active),
			MigrationCursor::Stuck => None,
		}
	}
}

impl<Cursor, BlockNumber> From<ActiveCursor<Cursor, BlockNumber>>
	for MigrationCursor<Cursor, BlockNumber>
{
	fn from(active: ActiveCursor<Cursor, BlockNumber>) -> Self {
		MigrationCursor::Active(active)
	}
}

/// Points to the currently active migration and its inner cursor.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo, MaxEncodedLen)]
pub struct ActiveCursor<Cursor, BlockNumber> {
	/// The index of the migration in the MBM tuple.
	pub index: u32,
	/// The cursor of the migration that is referenced by `index`.
	pub inner_cursor: Option<Cursor>,
	/// The block number that the migration started at.
	///
	/// This is used to calculate how many blocks it took.
	pub started_at: BlockNumber,
}

impl<Cursor, BlockNumber> ActiveCursor<Cursor, BlockNumber> {
	/// Advance the overarching cursor to the next migration.
	pub(crate) fn goto_next_migration(&mut self, current_block: BlockNumber) {
		self.index.saturating_inc();
		self.inner_cursor = None;
		self.started_at = current_block;
	}
}

/// How to clear the records of historic migrations.
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, scale_info::TypeInfo)]
pub enum HistoricCleanupSelector<Id> {
	/// Clear exactly these entries.
	///
	/// This is the advised way of doing it.
	Specific(Vec<Id>),

	/// Clear up to this many entries
	Wildcard {
		/// How many should be cleared in this call at most.
		limit: Option<u32>,
		/// The cursor that was emitted from any previous `HistoricCleared`.
		///
		/// Does not need to be passed when clearing the first batch.
		previous_cursor: Option<Vec<u8>>,
	},
}

/// The default number of entries that should be cleared by a `HistoricCleanupSelector::Wildcard`.
///
/// The caller can explicitly specify a higher amount. Benchmarks are run with twice this value.
const DEFAULT_HISTORIC_BATCH_CLEAR_SIZE: u32 = 128;

impl<Id> HistoricCleanupSelector<Id> {
	/// The maximal number of entries that this will remove.
	///
	/// Needed for weight calculation.
	pub fn limit(&self) -> u32 {
		match self {
			Self::Specific(ids) => ids.len() as u32,
			Self::Wildcard { limit, .. } => limit.unwrap_or(DEFAULT_HISTORIC_BATCH_CLEAR_SIZE),
		}
	}
}

/// Convenience alias for [`MigrationCursor`].
pub type CursorOf<T> = MigrationCursor<RawCursorOf<T>, BlockNumberFor<T>>;

/// Convenience alias for the raw inner cursor of a migration.
pub type RawCursorOf<T> = BoundedVec<u8, <T as Config>::CursorMaxLen>;

/// Convenience alias for the identifier of a migration.
pub type IdentifierOf<T> = BoundedVec<u8, <T as Config>::IdentifierMaxLen>;

/// Convenience alias for [`ActiveCursor`].
pub type ActiveCursorOf<T> = ActiveCursor<RawCursorOf<T>, BlockNumberFor<T>>;

/// Trait for a tuple of No-OP migrations with one element.
pub trait MockedMigrations: SteppedMigrations {
	/// The migration should fail after `n` steps.
	fn set_fail_after(n: u32);
	/// The migration should succeed after `n` steps.
	fn set_success_after(n: u32);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type of the runtime.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// All the multi-block migrations to run.
		///
		/// Should only be updated in a runtime-upgrade once all the old migrations have completed.
		/// (Check that [`Cursor`] is `None`).
		#[cfg(not(feature = "runtime-benchmarks"))]
		type Migrations: SteppedMigrations;

		/// Mocked migrations for benchmarking only.
		///
		/// Should be configured to [`crate::mock_helpers::MockedMigrations`] in benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type Migrations: MockedMigrations;

		/// The maximal length of an encoded cursor.
		///
		/// A good default needs to selected such that no migration will ever have a cursor with MEL
		/// above this limit. This is statically checked in `integrity_test`.
		#[pallet::constant]
		type CursorMaxLen: Get<u32>;

		/// The maximal length of an encoded identifier.
		///
		/// A good default needs to selected such that no migration will ever have an identifier
		/// with MEL above this limit. This is statically checked in `integrity_test`.
		#[pallet::constant]
		type IdentifierMaxLen: Get<u32>;

		/// Notifications for status updates of a runtime upgrade.
		///
		/// Could be used to pause XCM etc.
		type MigrationStatusHandler: MigrationStatusHandler;

		/// Handler for failed migrations.
		type FailedMigrationHandler: FailedMigrationHandler;

		/// The maximum weight to spend each block to execute migrations.
		type MaxServiceWeight: Get<Weight>;

		/// Weight information for the calls and functions of this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The currently active migration to run and its cursor.
	///
	/// `None` indicates that no migration is running.
	#[pallet::storage]
	pub type Cursor<T: Config> = StorageValue<_, CursorOf<T>, OptionQuery>;

	/// Set of all successfully executed migrations.
	///
	/// This is used as blacklist, to not re-execute migrations that have not been removed from the
	/// codebase yet. Governance can regularly clear this out via `clear_historic`.
	#[pallet::storage]
	pub type Historic<T: Config> = StorageMap<_, Twox64Concat, IdentifierOf<T>, (), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A Runtime upgrade started.
		///
		/// Its end is indicated by `UpgradeCompleted` or `UpgradeFailed`.
		UpgradeStarted {
			/// The number of migrations that this upgrade contains.
			///
			/// This can be used to design a progress indicator in combination with counting the
			/// `MigrationCompleted` and `MigrationSkipped` events.
			migrations: u32,
		},
		/// The current runtime upgrade completed.
		///
		/// This implies that all of its migrations completed successfully as well.
		UpgradeCompleted,
		/// Runtime upgrade failed.
		///
		/// This is very bad and will require governance intervention.
		UpgradeFailed,
		/// A migration was skipped since it was already executed in the past.
		MigrationSkipped {
			/// The index of the skipped migration within the [`Config::Migrations`] list.
			index: u32,
		},
		/// A migration progressed.
		MigrationAdvanced {
			/// The index of the migration within the [`Config::Migrations`] list.
			index: u32,
			/// The number of blocks that this migration took so far.
			took: BlockNumberFor<T>,
		},
		/// A Migration completed.
		MigrationCompleted {
			/// The index of the migration within the [`Config::Migrations`] list.
			index: u32,
			/// The number of blocks that this migration took so far.
			took: BlockNumberFor<T>,
		},
		/// A Migration failed.
		///
		/// This implies that the whole upgrade failed and governance intervention is required.
		MigrationFailed {
			/// The index of the migration within the [`Config::Migrations`] list.
			index: u32,
			/// The number of blocks that this migration took so far.
			took: BlockNumberFor<T>,
		},
		/// The set of historical migrations has been cleared.
		HistoricCleared {
			/// Should be passed to `clear_historic` in a successive call.
			next_cursor: Option<Vec<u8>>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The operation cannot complete since some MBMs are ongoing.
		Ongoing,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			Self::onboard_new_mbms()
		}

		#[cfg(feature = "std")]
		fn integrity_test() {
			// Check that the migrations tuple is legit.
			frame_support::assert_ok!(T::Migrations::integrity_test());

			// Very important! Ensure that the pallet is configured in `System::Config`.
			{
				assert!(!Cursor::<T>::exists(), "Externalities storage should be clean");
				assert!(!<T as frame_system::Config>::MultiBlockMigrator::ongoing());

				Cursor::<T>::put(MigrationCursor::Stuck);
				assert!(<T as frame_system::Config>::MultiBlockMigrator::ongoing());

				Cursor::<T>::kill();
			}

			// The per-block service weight is sane.
			#[cfg(not(test))]
			{
				let want = T::MaxServiceWeight::get();
				let max = <T as frame_system::Config>::BlockWeights::get().max_block;

				assert!(want.all_lte(max), "Service weight is larger than a block: {want} > {max}",);
			}

			// Cursor MEL
			{
				let mel = T::Migrations::cursor_max_encoded_len();
				let max_mel = T::CursorMaxLen::get() as usize;
				assert!(
					mel <= max_mel,
					"A Cursor is not guaranteed to fit into the storage: {mel} > {max_mel}",
				);
			}

			// Identifier MEL
			{
				let mel = T::Migrations::identifier_max_encoded_len();
				let max_mel = T::IdentifierMaxLen::get() as usize;
				assert!(
					mel <= max_mel,
					"An Identifier is not guaranteed to fit into the storage: {mel} > {max_mel}",
				);
			}
		}
	}

	#[pallet::call(weight = T::WeightInfo)]
	impl<T: Config> Pallet<T> {
		/// Allows root to set a cursor to forcefully start, stop or forward the migration process.
		///
		/// Should normally not be needed and is only in place as emergency measure. Note that
		/// restarting the migration process in this manner will not call the
		/// [`MigrationStatusHandler::started`] hook or emit an `UpgradeStarted` event.
		#[pallet::call_index(0)]
		pub fn force_set_cursor(
			origin: OriginFor<T>,
			cursor: Option<CursorOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			Cursor::<T>::set(cursor);

			Ok(())
		}

		/// Allows root to set an active cursor to forcefully start/forward the migration process.
		///
		/// This is an edge-case version of [`Self::force_set_cursor`] that allows to set the
		/// `started_at` value to the next block number. Otherwise this would not be possible, since
		/// `force_set_cursor` takes an absolute block number. Setting `started_at` to `None`
		/// indicates that the current block number plus one should be used.
		#[pallet::call_index(1)]
		pub fn force_set_active_cursor(
			origin: OriginFor<T>,
			index: u32,
			inner_cursor: Option<RawCursorOf<T>>,
			started_at: Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let started_at = started_at.unwrap_or(
				System::<T>::block_number().saturating_add(sp_runtime::traits::One::one()),
			);
			Cursor::<T>::put(MigrationCursor::Active(ActiveCursor {
				index,
				inner_cursor,
				started_at,
			}));

			Ok(())
		}

		/// Forces the onboarding of the migrations.
		///
		/// This process happens automatically on a runtime upgrade. It is in place as an emergency
		/// measurement. The cursor needs to be `None` for this to succeed.
		#[pallet::call_index(2)]
		pub fn force_onboard_mbms(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;

			ensure!(!Cursor::<T>::exists(), Error::<T>::Ongoing);
			Self::onboard_new_mbms();

			Ok(())
		}

		/// Clears the `Historic` set.
		///
		/// `map_cursor` must be set to the last value that was returned by the
		/// `HistoricCleared` event. The first time `None` can be used. `limit` must be chosen in a
		/// way that will result in a sensible weight.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::clear_historic(selector.limit()))]
		pub fn clear_historic(
			origin: OriginFor<T>,
			selector: HistoricCleanupSelector<IdentifierOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			match &selector {
				HistoricCleanupSelector::Specific(ids) => {
					for id in ids {
						Historic::<T>::remove(id);
					}
					Self::deposit_event(Event::HistoricCleared { next_cursor: None });
				},
				HistoricCleanupSelector::Wildcard { previous_cursor, .. } => {
					let next = Historic::<T>::clear(selector.limit(), previous_cursor.as_deref());
					Self::deposit_event(Event::HistoricCleared { next_cursor: next.maybe_cursor });
				},
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Onboard all new Multi-Block-Migrations and start the process of executing them.
	///
	/// Should only be called once all previous migrations completed.
	fn onboard_new_mbms() -> Weight {
		if let Some(cursor) = Cursor::<T>::get() {
			log::error!("Ongoing migrations interrupted - chain stuck");

			let maybe_index = cursor.as_active().map(|c| c.index);
			Self::upgrade_failed(maybe_index);
			return T::WeightInfo::onboard_new_mbms()
		}

		let migrations = T::Migrations::len();
		log::debug!("Onboarding {migrations} new MBM migrations");

		if migrations > 0 {
			// Set the cursor to the first migration:
			Cursor::<T>::set(Some(
				ActiveCursor {
					index: 0,
					inner_cursor: None,
					started_at: System::<T>::block_number(),
				}
				.into(),
			));
			Self::deposit_event(Event::UpgradeStarted { migrations });
			T::MigrationStatusHandler::started();
		}

		T::WeightInfo::onboard_new_mbms()
	}

	/// Tries to make progress on the Multi-Block-Migrations process.
	fn progress_mbms(n: BlockNumberFor<T>) -> Weight {
		let mut meter = WeightMeter::with_limit(T::MaxServiceWeight::get());
		meter.consume(T::WeightInfo::progress_mbms_none());

		let mut cursor = match Cursor::<T>::get() {
			None => {
				log::trace!("[Block {n:?}] Waiting for cursor to become `Some`.");
				return meter.consumed()
			},
			Some(MigrationCursor::Active(cursor)) => {
				log::debug!("Progressing MBM #{}", cursor.index);
				cursor
			},
			Some(MigrationCursor::Stuck) => {
				log::error!("Migration stuck. Governance intervention required.");
				return meter.consumed()
			},
		};
		debug_assert!(Self::ongoing());

		// The limit here is a defensive measure to prevent an infinite loop. It expresses that we
		// allow no more than 8 MBMs to finish in a single block. This should be harmless, since we
		// generally expect *Multi*-Block-Migrations to take *multiple* blocks.
		for i in 0..8 {
			match Self::exec_migration(cursor, i == 0, &mut meter) {
				None => return meter.consumed(),
				Some(ControlFlow::Continue(next_cursor)) => {
					cursor = next_cursor;
				},
				Some(ControlFlow::Break(last_cursor)) => {
					cursor = last_cursor;
					break
				},
			}
		}

		Cursor::<T>::set(Some(cursor.into()));

		meter.consumed()
	}

	/// Try to make progress on the current migration.
	///
	/// Returns whether processing should continue or break for this block. The return value means:
	/// - `None`: The migration process is completely finished.
	/// - `ControlFlow::Break`: Continue in the *next* block with the given cursor.
	/// - `ControlFlow::Continue`: Continue in the *current* block with the given cursor.
	fn exec_migration(
		mut cursor: ActiveCursorOf<T>,
		is_first: bool,
		meter: &mut WeightMeter,
	) -> Option<ControlFlow<ActiveCursorOf<T>, ActiveCursorOf<T>>> {
		// The differences between the single branches' weights is not that big. And since we do
		// only one step per block, we can just use the maximum instead of more precise accounting.
		if meter.try_consume(Self::exec_migration_max_weight()).is_err() {
			defensive_assert!(!is_first, "There should be enough weight to do this at least once");
			return Some(ControlFlow::Break(cursor))
		}

		let Some(id) = T::Migrations::nth_id(cursor.index) else {
			// No more migrations in the tuple - we are done.
			defensive_assert!(cursor.index == T::Migrations::len(), "Inconsistent MBMs tuple");
			Self::deposit_event(Event::UpgradeCompleted);
			Cursor::<T>::kill();
			T::MigrationStatusHandler::completed();
			return None;
		};

		let Ok(bounded_id): Result<IdentifierOf<T>, _> = id.try_into() else {
			defensive!("integrity_test ensures that all identifiers' MEL bounds fit into CursorMaxLen; qed.");
			Self::upgrade_failed(Some(cursor.index));
			return None
		};

		if Historic::<T>::contains_key(&bounded_id) {
			Self::deposit_event(Event::MigrationSkipped { index: cursor.index });
			cursor.goto_next_migration(System::<T>::block_number());
			return Some(ControlFlow::Continue(cursor))
		}

		let max_steps = T::Migrations::nth_max_steps(cursor.index);
		let next_cursor = T::Migrations::nth_transactional_step(
			cursor.index,
			cursor.inner_cursor.clone().map(|c| c.into_inner()),
			meter,
		);
		let Some((max_steps, next_cursor)) = max_steps.zip(next_cursor) else {
			defensive!("integrity_test ensures that the tuple is valid; qed");
			Self::upgrade_failed(Some(cursor.index));
			return None
		};

		let took = System::<T>::block_number().saturating_sub(cursor.started_at);
		match next_cursor {
			Ok(Some(next_cursor)) => {
				let Ok(bound_next_cursor) = next_cursor.try_into() else {
					defensive!("The integrity check ensures that all cursors' MEL bound fits into CursorMaxLen; qed");
					Self::upgrade_failed(Some(cursor.index));
					return None
				};

				Self::deposit_event(Event::MigrationAdvanced { index: cursor.index, took });
				cursor.inner_cursor = Some(bound_next_cursor);

				if max_steps.map_or(false, |max| took > max.into()) {
					Self::deposit_event(Event::MigrationFailed { index: cursor.index, took });
					Self::upgrade_failed(Some(cursor.index));
					None
				} else {
					// A migration cannot progress more than one step per block, we therefore break.
					Some(ControlFlow::Break(cursor))
				}
			},
			Ok(None) => {
				// A migration is done when it returns cursor `None`.
				Self::deposit_event(Event::MigrationCompleted { index: cursor.index, took });
				Historic::<T>::insert(&bounded_id, ());
				cursor.goto_next_migration(System::<T>::block_number());
				Some(ControlFlow::Continue(cursor))
			},
			Err(SteppedMigrationError::InsufficientWeight { required }) => {
				if is_first || required.any_gt(meter.limit()) {
					Self::deposit_event(Event::MigrationFailed { index: cursor.index, took });
					Self::upgrade_failed(Some(cursor.index));
					None
				} else {
					// Retry and hope that there is more weight in the next block.
					Some(ControlFlow::Break(cursor))
				}
			},
			Err(SteppedMigrationError::InvalidCursor | SteppedMigrationError::Failed) => {
				Self::deposit_event(Event::MigrationFailed { index: cursor.index, took });
				Self::upgrade_failed(Some(cursor.index));
				None
			},
		}
	}

	/// Fail the current runtime upgrade, caused by `migration`.
	fn upgrade_failed(migration: Option<u32>) {
		use FailedMigrationHandling::*;
		Self::deposit_event(Event::UpgradeFailed);

		match T::FailedMigrationHandler::failed(migration) {
			KeepStuck => Cursor::<T>::set(Some(MigrationCursor::Stuck)),
			ForceUnstuck => Cursor::<T>::kill(),
			Ignore => {},
		}
	}

	fn exec_migration_max_weight() -> Weight {
		T::WeightInfo::exec_migration_complete()
			.max(T::WeightInfo::exec_migration_completed())
			.max(T::WeightInfo::exec_migration_skipped_historic())
			.max(T::WeightInfo::exec_migration_advance())
			.max(T::WeightInfo::exec_migration_fail())
	}
}

impl<T: Config> MultiStepMigrator for Pallet<T> {
	fn ongoing() -> bool {
		Cursor::<T>::exists()
	}

	fn step() -> Weight {
		Self::progress_mbms(System::<T>::block_number())
	}
}
