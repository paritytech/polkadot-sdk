// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! # Linked-list pallet
//!
//! A generic per-list sorted doubly-linked list. Items live in independent
//! lists keyed by `ListId`; within a list they are kept in strict priority order,
//! head (highest priority) to tail (lowest). Same-priority items land on the tail
//! side of their cluster, so tail-first iteration is LIFO within a priority
//! cluster.
//!
//! Insertion accepts a [`Position`] hint (a typed `(prev, next)` pair where
//! endpoints are encoded as `None`) and repairs stale hints on-chain up to
//! `MaxHintRepairSteps`.
//!
//! ## Overview
//!
//! Consumer pallets use the [`SortedListInterface`] trait. The single
//! dispatchable, [`Pallet::reprioritize`], is permissionless and re-fetches an
//! item's authoritative priority from [`PriorityProvider`] to correct drift.
//!
//! ## Interface
//!
//! - [`SortedListInterface::insert`]: O(1) with valid hints, otherwise a bounded repair walk.
//! - [`SortedListInterface::remove`]: O(1) splice.
//! - [`SortedListInterface::pop_tail`]: O(1) tail pop for LIFO consumers.
//! - [`SortedListInterface::re_insert`]: in-place when the existing position still admits the new
//!   priority, otherwise splice + repair + re-insert.
//! - [`SortedListInterface::iter_from_tail`]: bounded tail-first iteration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame::prelude::*;

pub use list::Node;
pub use pallet::*;
pub use sorted_list_interface::{PriorityProvider, SortedListInterface};
pub use types::{Position, Side};

/// Benchmark fixture: overrides the authoritative priority used by
/// [`PriorityProvider`] so the `reprioritize` benchmark can simulate priority drift.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<ListId, ItemId, Priority> {
	fn set_priority(list_id: &ListId, item: &ItemId, priority: Priority);
}

mod dispatchables;
mod list;
mod sorted_list_interface;
mod try_state;
mod types;
mod view_helpers;
pub mod weights;

pub(crate) const LOG_TARGET: &str = "runtime::linked-list";

// Syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		frame::log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[{:?}] [{}] ", $patter),
			<frame_system::Pallet<T>>::block_number(),
			<$crate::Pallet::<T> as frame::deps::frame_support::traits::PalletInfoAccess>::name()
			$(, $values)*
		)
	};
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;

	pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Outer key partitioning the lists.
		type ListId: Parameter + Member + MaxEncodedLen + Copy;

		/// Inner key identifying an item within a list.
		type ItemId: Parameter + Member + MaxEncodedLen + Copy;

		/// Sort key. Higher priorities are closer to the head, lower priorities closer
		/// to the tail.
		type Priority: Parameter + Member + Copy + Ord + MaxEncodedLen;

		/// Authoritative source of an item's priority. Consulted by
		/// [`Pallet::reprioritize`] to detect drift.
		type PriorityProvider: PriorityProvider<
			Self::ListId,
			Self::ItemId,
			Priority = Self::Priority,
		>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: weights::WeightInfo;

		/// Maximum nodes the on-chain hint-repair walk may traverse before
		/// failing with [`Error::InvalidPositionHints`].
		#[pallet::constant]
		type MaxHintRepairSteps: Get<u32>;

		/// Benchmark fixture used to mint test values.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::ListId, Self::ItemId, Self::Priority>;
	}

	/// Nodes of the per-list sorted list.
	#[pallet::storage]
	pub type ListNodes<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::ListId,
		Blake2_128Concat,
		T::ItemId,
		Node<T::ItemId, T::Priority>,
		OptionQuery,
	>;

	/// Highest-priority item in each non-empty list.
	#[pallet::storage]
	pub type ListHeads<T: Config> = StorageMap<_, Twox64Concat, T::ListId, T::ItemId, OptionQuery>;

	/// Lowest-priority item in each non-empty list.
	#[pallet::storage]
	pub type ListTails<T: Config> = StorageMap<_, Twox64Concat, T::ListId, T::ItemId, OptionQuery>;

	/// Node count per list. Removed (not zeroed) when a list empties.
	#[pallet::storage]
	pub type ListSizes<T: Config> = StorageMap<_, Twox64Concat, T::ListId, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An item was inserted into a list.
		ItemInserted { list_id: T::ListId, item: T::ItemId, priority: T::Priority },
		/// An item was removed from a list.
		ItemRemoved { list_id: T::ListId, item: T::ItemId },
		/// An item's priority was changed.
		ItemReinserted {
			list_id: T::ListId,
			item: T::ItemId,
			old_priority: T::Priority,
			new_priority: T::Priority,
		},
		/// An item was reprioritized after its authoritative priority drifted.
		Reprioritized { list_id: T::ListId, item: T::ItemId, new_priority: T::Priority },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// `(list_id, item)` is not in the list.
		ItemNotFound,
		/// `(list_id, item)` is already in the list.
		ItemAlreadyExists,
		/// The list's size counter cannot represent one more item.
		ListTooLong,
		/// Stored links or counters are internally inconsistent.
		CorruptList,
		/// The supplied hint could not be repaired within `MaxHintRepairSteps`.
		InvalidPositionHints,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(T::MaxHintRepairSteps::get() > 0, "`MaxHintRepairSteps` must be > 0");
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), frame::try_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		/// Highest-priority item in `list_id`, or `None` if empty.
		pub fn head(list_id: T::ListId) -> Option<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::head(&list_id)
		}

		/// Lowest-priority item in `list_id`, or `None` if empty.
		pub fn tail(list_id: T::ListId) -> Option<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::tail(&list_id)
		}

		/// Number of items in `list_id`.
		pub fn count(list_id: T::ListId) -> u32 {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::count(&list_id)
		}

		/// Whether `(list_id, item)` is currently in the list.
		pub fn contains(list_id: T::ListId, item: T::ItemId) -> bool {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::contains(&list_id, &item)
		}

		/// Current `(prev, next)` neighbors of `(list_id, item)`, if present.
		pub fn neighbors(list_id: T::ListId, item: T::ItemId) -> Option<Position<T::ItemId>> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::neighbors(&list_id, &item)
		}

		/// Stored priority cached on `(list_id, item)`'s node, or `None` if the
		/// item is not in the list.
		pub fn priority(list_id: T::ListId, item: T::ItemId) -> Option<T::Priority> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::priority(&list_id, &item)
		}

		/// First `n` items of `list_id` walking from the tail. Returns fewer
		/// than `n` if the list has fewer items.
		pub fn iter_from_tail(list_id: T::ListId, n: u32) -> alloc::vec::Vec<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::iter_from_tail(&list_id, n)
		}

		/// Insertion [`Position`] for `priority` in `list_id`.
		pub fn find_position(list_id: T::ListId, priority: T::Priority) -> Position<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::find_position(&list_id, priority)
		}

		/// Position `(list_id, item)` should occupy at `new_priority`. Returns
		/// `None` if the item is not in the list.
		pub fn find_re_insert_position(
			list_id: T::ListId,
			item: T::ItemId,
			new_priority: T::Priority,
		) -> Option<Position<T::ItemId>> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::find_re_insert_position(
				&list_id,
				&item,
				new_priority,
			)
		}

		/// Steps the on-chain repair walk would take from `hint` to reach the
		/// position for `priority`. Returns `0` if the hint is already valid,
		/// or a value greater than `MaxHintRepairSteps` if the call would fail.
		pub fn repair_steps_needed(
			list_id: T::ListId,
			priority: T::Priority,
			hint: Position<T::ItemId>,
		) -> u32 {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::repair_steps_needed(
				&list_id, priority, hint,
			)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reposition `(list_id, item)` after its authoritative priority, fetched
		/// from [`PriorityProvider`], has drifted from the stored priority.
		///
		/// Anyone can call this. The caller supplies a [`Position`] hint for
		/// the new position; stale hints are repaired up to
		/// `MaxHintRepairSteps`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::reprioritize(T::MaxHintRepairSteps::get()))]
		pub fn reprioritize(
			origin: OriginFor<T>,
			list_id: T::ListId,
			item: T::ItemId,
			hint: Position<T::ItemId>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			let actual_weight = Self::do_reprioritize(list_id, item, hint)?;
			Ok(Some(actual_weight).into())
		}
	}
}
