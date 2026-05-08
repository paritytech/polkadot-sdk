//! Consumer-facing trait surface for the sorted list.

use crate::{list, pallet::*, view_helpers};
use alloc::vec::Vec;
use frame::deps::frame_support::{
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::DefensiveOption,
};

/// Authoritative source of the score for `(list_id, item)`. Consulted by
/// [`crate::Pallet::relist`] to detect drift against stored node scores.
pub trait ScoreProvider<ListId, ItemId> {
	/// Score type used to order items.
	type Score;

	/// Current authoritative score for `(list_id, item)`.
	///
	/// Returns `None` when the item should not remain in the list.
	fn score(list_id: &ListId, item: &ItemId) -> Option<Self::Score>;
}

/// Mutation and query surface for consumer pallets.
///
/// Position hints are `(prev, next)` pairs; endpoints are `None`. Mutating
/// methods return the number of hint-repair steps actually walked so callers
/// can refund unused weight via `PostDispatchInfo::actual_weight`.
pub trait SortedListInterface<ListId, ItemId> {
	/// Score type used to order items within a list.
	type Score;

	/// Error type returned by mutating operations.
	type Error;

	/// Insert `(list_id, item)` at `score`, repairing stale hints if needed.
	///
	/// # Errors
	///
	/// - `ItemAlreadyExists` if `(list_id, item)` is already in the list.
	/// - `ListTooLong` if the list's size counter would overflow.
	/// - `InvalidPositionHints` if the hint cannot be repaired within the budget.
	fn insert(
		list_id: ListId,
		item: ItemId,
		score: Self::Score,
		hint_prev: Option<ItemId>,
		hint_next: Option<ItemId>,
	) -> Result<u32, Self::Error>;

	/// Remove `(list_id, item)`.
	///
	/// # Errors
	///
	/// - `ItemNotFound` if `(list_id, item)` is not in the list.
	/// - `CorruptList` if the node exists but list metadata is inconsistent.
	fn remove(list_id: &ListId, item: &ItemId) -> Result<(), Self::Error>;

	/// Remove and return the current tail item of `list_id`, or `None` if the
	/// list is empty.
	///
	/// This is the LIFO primitive for consumers that insert equal-score items
	/// and consume from the tail.
	///
	/// # Errors
	///
	/// - `CorruptList` if the tail pointer or list metadata is inconsistent.
	fn pop_tail(list_id: &ListId) -> Result<Option<(ItemId, Self::Score)>, Self::Error>;

	/// Re-insert `(list_id, item)` at `new_score`. Updates the score in place
	/// when the existing neighbors still admit it; otherwise splices the item
	/// out and re-inserts at the hint.
	///
	/// # Errors
	///
	/// - `ItemNotFound` if `(list_id, item)` is not in the list.
	/// - `ListTooLong` if the list's size counter would overflow.
	/// - `CorruptList` if the node exists but list metadata is inconsistent.
	/// - `InvalidPositionHints` if the hint cannot be repaired within the budget.
	fn re_insert(
		list_id: ListId,
		item: ItemId,
		new_score: Self::Score,
		hint_prev: Option<ItemId>,
		hint_next: Option<ItemId>,
	) -> Result<u32, Self::Error>;

	/// Highest-score item in `list_id`, or `None` if empty.
	fn head(list_id: &ListId) -> Option<ItemId>;

	/// Lowest-score item in `list_id`, or `None` if empty.
	fn tail(list_id: &ListId) -> Option<ItemId>;

	/// Number of items in `list_id`.
	fn count(list_id: &ListId) -> u32;

	/// Returns `true` if `(list_id, item)` is in the list.
	fn contains(list_id: &ListId, item: &ItemId) -> bool;

	/// Current `(prev, next)` neighbors of `(list_id, item)`, if present.
	fn neighbors(list_id: &ListId, item: &ItemId) -> Option<(Option<ItemId>, Option<ItemId>)>;

	/// Stored score cached on `(list_id, item)`'s node, or `None` if absent.
	fn score(list_id: &ListId, item: &ItemId) -> Option<Self::Score>;

	/// First `n` items of `list_id` walking from the tail. Returns fewer than
	/// `n` if the list has fewer items.
	fn iter_from_tail(list_id: &ListId, n: u32) -> Vec<ItemId>;

	/// `(prev, next)` insertion position for `score` in `list_id`.
	///
	/// Endpoints are returned as `None`. O(list size); intended for hint
	/// preparation, not hot paths.
	fn find_position(list_id: &ListId, score: Self::Score) -> (Option<ItemId>, Option<ItemId>);

	/// `(prev, next)` position `(list_id, item)` should occupy at `new_score`,
	/// skipping the item's own node.
	///
	/// Returns `None` if the item is not in the list. O(list size); intended
	/// for hint preparation, not hot paths.
	fn find_re_insert_position(
		list_id: &ListId,
		item: &ItemId,
		new_score: Self::Score,
	) -> Option<(Option<ItemId>, Option<ItemId>)>;

	/// Steps needed to repair `(hint_prev, hint_next)` for `score` in
	/// `list_id`.
	///
	/// Returns `0` if the hint is already valid, or a value greater than
	/// `MaxHintRepairSteps` if the same dispatch would fail.
	fn repair_steps_needed(
		list_id: &ListId,
		score: Self::Score,
		hint_prev: Option<ItemId>,
		hint_next: Option<ItemId>,
	) -> u32;
}

impl<T: Config> SortedListInterface<T::ListId, T::ItemId> for Pallet<T> {
	type Score = T::Score;
	type Error = Error<T>;

	fn insert(
		list_id: T::ListId,
		item: T::ItemId,
		score: T::Score,
		hint_prev: Option<T::ItemId>,
		hint_next: Option<T::ItemId>,
	) -> Result<u32, Error<T>> {
		if ListNodes::<T>::contains_key(&list_id, &item) {
			return Err(Error::<T>::ItemAlreadyExists);
		}
		let (prev, next, steps) = list::walk_repair::<T>(&list_id, &score, hint_prev, hint_next)?;
		list::insert_at::<T>(&list_id, &item, score, prev, next)?;
		Self::deposit_event(Event::ItemInserted { list_id, item, score });
		Ok(steps)
	}

	fn remove(list_id: &T::ListId, item: &T::ItemId) -> Result<(), Error<T>> {
		list::remove_at::<T>(list_id, item)?;
		Self::deposit_event(Event::ItemRemoved { list_id: list_id.clone(), item: item.clone() });
		Ok(())
	}

	fn pop_tail(list_id: &T::ListId) -> Result<Option<(T::ItemId, T::Score)>, Error<T>> {
		let Some(item) = ListTails::<T>::get(list_id) else { return Ok(None) };
		// Defensive: by `try_state` invariant 1, `ListTails` always points to a
		// present node.
		let score = ListNodes::<T>::get(list_id, &item)
			.defensive_ok_or(Error::<T>::CorruptList)?
			.score;
		list::remove_at::<T>(list_id, &item)?;
		Self::deposit_event(Event::ItemRemoved { list_id: list_id.clone(), item: item.clone() });
		Ok(Some((item, score)))
	}

	fn re_insert(
		list_id: T::ListId,
		item: T::ItemId,
		new_score: T::Score,
		hint_prev: Option<T::ItemId>,
		hint_next: Option<T::ItemId>,
	) -> Result<u32, Error<T>> {
		let existing = ListNodes::<T>::get(&list_id, &item).ok_or(Error::<T>::ItemNotFound)?;
		let old_score = existing.score;

		// Fast path: same score.
		if old_score == new_score {
			return Ok(0);
		}

		// Fast path: existing neighbors still admit the new score, mutate in place.
		if list::neighbor_scores_admit::<T>(&list_id, &new_score, &existing.prev, &existing.next) {
			ListNodes::<T>::mutate(&list_id, &item, |maybe| {
				if let Some(n) = maybe {
					n.score = new_score;
				}
			});
			Self::deposit_event(Event::ItemReinserted { list_id, item, old_score, new_score });
			return Ok(0);
		}

		// Slow path: splice + re-insert. Wrapped in a nested storage layer so
		// that an `InvalidPositionHints` after `remove_at` rolls back cleanly.
		let outer = with_transaction_opaque_err::<u32, Error<T>, _>(|| {
			let inner = (|| -> Result<u32, Error<T>> {
				list::remove_at::<T>(&list_id, &item)?;
				let (prev, next, steps) =
					list::walk_repair::<T>(&list_id, &new_score, hint_prev, hint_next)?;
				list::insert_at::<T>(&list_id, &item, new_score, prev, next)?;
				Ok(steps)
			})();
			if inner.is_ok() {
				TransactionOutcome::Commit(inner)
			} else {
				TransactionOutcome::Rollback(inner)
			}
		});
		// `Err(())` only fires on transactional-layer nesting overflow.
		let steps = outer.map_err(|()| Error::<T>::InvalidPositionHints)??;
		Self::deposit_event(Event::ItemReinserted { list_id, item, old_score, new_score });
		Ok(steps)
	}

	fn head(list_id: &T::ListId) -> Option<T::ItemId> {
		ListHeads::<T>::get(list_id)
	}

	fn tail(list_id: &T::ListId) -> Option<T::ItemId> {
		ListTails::<T>::get(list_id)
	}

	fn count(list_id: &T::ListId) -> u32 {
		ListSizes::<T>::get(list_id)
	}

	fn contains(list_id: &T::ListId, item: &T::ItemId) -> bool {
		ListNodes::<T>::contains_key(list_id, item)
	}

	fn neighbors(
		list_id: &T::ListId,
		item: &T::ItemId,
	) -> Option<(Option<T::ItemId>, Option<T::ItemId>)> {
		ListNodes::<T>::get(list_id, item).map(|n| (n.prev, n.next))
	}

	fn score(list_id: &T::ListId, item: &T::ItemId) -> Option<T::Score> {
		ListNodes::<T>::get(list_id, item).map(|n| n.score)
	}

	fn iter_from_tail(list_id: &T::ListId, n: u32) -> Vec<T::ItemId> {
		view_helpers::iter_from_tail::<T>(list_id, n)
	}

	fn find_position(
		list_id: &T::ListId,
		score: T::Score,
	) -> (Option<T::ItemId>, Option<T::ItemId>) {
		view_helpers::find_position::<T>(list_id, score)
	}

	fn find_re_insert_position(
		list_id: &T::ListId,
		item: &T::ItemId,
		new_score: T::Score,
	) -> Option<(Option<T::ItemId>, Option<T::ItemId>)> {
		view_helpers::find_re_insert_position::<T>(list_id, item, new_score)
	}

	fn repair_steps_needed(
		list_id: &T::ListId,
		score: T::Score,
		hint_prev: Option<T::ItemId>,
		hint_next: Option<T::ItemId>,
	) -> u32 {
		view_helpers::repair_steps_needed::<T>(list_id, score, hint_prev, hint_next)
	}
}
