//! # Linked-list pallet
//!
//! A generic per-list sorted doubly-linked list. Items live in independent
//! lists keyed by `ListId`; within a list they are kept in strict score order,
//! head (highest score) to tail (lowest). Same-score items land on the tail
//! side of their cluster, so tail-first iteration is LIFO within a score
//! cluster.
//!
//! Insertion accepts a `(prev, next)` hint and repairs stale hints on-chain up
//! to `MaxHintRepairSteps`. Endpoints are encoded as `None`.
//!
//! ## Overview
//!
//! Consumer pallets use the [`SortedListInterface`] trait. The single
//! dispatchable, [`Pallet::relist`], is permissionless and re-fetches an
//! item's authoritative score from [`ScoreProvider`] to correct drift.
//!
//! ## Interface
//!
//! - [`SortedListInterface::insert`]: O(1) with valid hints, otherwise a bounded repair walk.
//! - [`SortedListInterface::remove`]: O(1) splice.
//! - [`SortedListInterface::pop_tail`]: O(1) tail pop for LIFO consumers.
//! - [`SortedListInterface::re_insert`]: in-place when the existing position still admits the new
//!   score, otherwise splice + repair + re-insert.
//! - [`SortedListInterface::iter_from_tail`]: bounded tail-first iteration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame::prelude::*;

pub use list::Node;
pub use pallet::*;
pub use sorted_list_interface::{ScoreProvider, SortedListInterface};

/// Benchmark fixture: overrides the authoritative score used by
/// [`ScoreProvider`] so the `relist` benchmark can simulate score drift.
///
/// `ListId`/`ItemId`/`Score` values are minted directly via `From<u32>` bounds
/// in the benchmark module; no helper indirection needed.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<ListId, ItemId, Score> {
	fn set_score(list_id: &ListId, item: &ItemId, score: Score);
}

mod dispatchables;
mod list;
mod sorted_list_interface;
mod try_state;
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

#[cfg(any(test, feature = "runtime-benchmarks"))]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo as _;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Outer key partitioning the lists.
		type ListId: Parameter + Member + MaxEncodedLen;

		/// Inner key identifying an item within a list.
		type ItemId: Parameter + Member + MaxEncodedLen;

		/// Sort key. Higher scores are closer to the head, lower scores closer
		/// to the tail.
		type Score: Parameter + Member + Copy + Ord + MaxEncodedLen;

		/// Authoritative source of an item's score. Consulted by
		/// [`Pallet::relist`] to detect drift.
		type ScoreProvider: ScoreProvider<Self::ListId, Self::ItemId, Score = Self::Score>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: weights::WeightInfo;

		/// Maximum nodes the on-chain hint-repair walk may traverse before
		/// failing with [`Error::InvalidPositionHints`].
		#[pallet::constant]
		type MaxHintRepairSteps: Get<u32>;

		/// Benchmark fixture used to mint test values.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::ListId, Self::ItemId, Self::Score>;
	}

	/// Nodes of the per-list sorted list.
	#[pallet::storage]
	pub type ListNodes<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		T::ListId,
		Blake2_128Concat,
		T::ItemId,
		Node<T::ItemId, T::Score>,
		OptionQuery,
	>;

	/// Highest-score item in each non-empty list.
	#[pallet::storage]
	pub type ListHeads<T: Config> = StorageMap<_, Twox64Concat, T::ListId, T::ItemId, OptionQuery>;

	/// Lowest-score item in each non-empty list.
	#[pallet::storage]
	pub type ListTails<T: Config> = StorageMap<_, Twox64Concat, T::ListId, T::ItemId, OptionQuery>;

	/// Node count per list. Removed (not zeroed) when a list empties.
	#[pallet::storage]
	pub type ListSizes<T: Config> = StorageMap<_, Twox64Concat, T::ListId, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An item was inserted into a list.
		ItemInserted { list_id: T::ListId, item: T::ItemId, score: T::Score },
		/// An item was removed from a list.
		ItemRemoved { list_id: T::ListId, item: T::ItemId },
		/// An item's score was changed.
		ItemReinserted {
			list_id: T::ListId,
			item: T::ItemId,
			old_score: T::Score,
			new_score: T::Score,
		},
		/// An item was relisted after its authoritative score drifted.
		Relisted { list_id: T::ListId, item: T::ItemId, new_score: T::Score },
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
		/// Highest-score item in `list_id`, or `None` if empty.
		pub fn head(list_id: T::ListId) -> Option<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::head(&list_id)
		}

		/// Lowest-score item in `list_id`, or `None` if empty.
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
		pub fn neighbors(
			list_id: T::ListId,
			item: T::ItemId,
		) -> Option<(Option<T::ItemId>, Option<T::ItemId>)> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::neighbors(&list_id, &item)
		}

		/// Stored score cached on `(list_id, item)`'s node, or `None` if the
		/// item is not in the list.
		pub fn score(list_id: T::ListId, item: T::ItemId) -> Option<T::Score> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::score(&list_id, &item)
		}

		/// First `n` items of `list_id` walking from the tail. Returns fewer
		/// than `n` if the list has fewer items.
		pub fn iter_from_tail(list_id: T::ListId, n: u32) -> alloc::vec::Vec<T::ItemId> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::iter_from_tail(&list_id, n)
		}

		/// `(prev, next)` insertion position for `score` in `list_id`.
		/// Endpoints are returned as `None`.
		pub fn find_position(
			list_id: T::ListId,
			score: T::Score,
		) -> (Option<T::ItemId>, Option<T::ItemId>) {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::find_position(&list_id, score)
		}

		/// `(prev, next)` position `(list_id, item)` should occupy at
		/// `new_score`. Returns `None` if the item is not in the list.
		pub fn find_re_insert_position(
			list_id: T::ListId,
			item: T::ItemId,
			new_score: T::Score,
		) -> Option<(Option<T::ItemId>, Option<T::ItemId>)> {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::find_re_insert_position(
				&list_id, &item, new_score,
			)
		}

		/// Steps the on-chain repair walk would take from
		/// `(hint_prev, hint_next)` to reach the position for `score`. Returns
		/// `0` if the hint is already valid, or a value greater than
		/// `MaxHintRepairSteps` if the call would fail.
		pub fn repair_steps_needed(
			list_id: T::ListId,
			score: T::Score,
			hint_prev: Option<T::ItemId>,
			hint_next: Option<T::ItemId>,
		) -> u32 {
			<Self as SortedListInterface<T::ListId, T::ItemId>>::repair_steps_needed(
				&list_id, score, hint_prev, hint_next,
			)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reposition `(list_id, item)` after its authoritative score, fetched
		/// from [`ScoreProvider`], has drifted from the stored score.
		///
		/// Anyone can call this. The caller supplies a `(hint_prev, hint_next)`
		/// for the new position; stale hints are repaired up to
		/// `MaxHintRepairSteps`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::relist(T::MaxHintRepairSteps::get()))]
		pub fn relist(
			origin: OriginFor<T>,
			list_id: T::ListId,
			item: T::ItemId,
			hint_prev: Option<T::ItemId>,
			hint_next: Option<T::ItemId>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			let steps =
				crate::dispatchables::relist_internal::<T>(&list_id, &item, hint_prev, hint_next)?;
			Ok(Some(T::WeightInfo::relist(steps)).into())
		}
	}
}
