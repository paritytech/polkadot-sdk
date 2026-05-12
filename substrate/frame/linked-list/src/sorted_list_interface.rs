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

//! Consumer-facing trait surface for the sorted list.

use crate::{list, pallet::*, view_helpers, Outcome, Position};
use alloc::vec::Vec;
use frame::deps::frame_support::{
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::DefensiveOption,
};

/// Authoritative source of the priority for `(list_id, item)`. Consulted by
/// [`crate::Pallet::reprioritize`] to detect drift against stored node priorities.
pub trait PriorityProvider<ListId, ItemId> {
	/// Priority type used to order items.
	type Priority;

	/// Current authoritative priority for `(list_id, item)`.
	///
	/// Returns `None` when the item should not remain in the list.
	fn priority(list_id: &ListId, item: &ItemId) -> Option<Self::Priority>;
}

/// Mutation and query surface for consumer pallets.
///
/// Position hints are [`Position`] values; endpoints are encoded as `None` in
/// each field. Mutating methods return the number of hint-repair steps
/// actually walked so callers can refund unused weight via
/// `PostDispatchInfo::actual_weight`.
pub trait SortedListInterface<ListId, ItemId> {
	/// Priority type used to order items within a list.
	type Priority;

	/// Error type returned by mutating operations.
	type Error;

	/// Insert `(list_id, item)` at `priority`, repairing stale hints if needed.
	///
	/// # Errors
	///
	/// - `ItemAlreadyExists` if `(list_id, item)` is already in the list.
	/// - `ListTooLong` if the list's size counter would overflow.
	/// - `InvalidPositionHints` if the hint cannot be repaired within the budget.
	/// - `CorruptList` if a hinted neighbor row is missing or its links are inconsistent.
	fn insert(
		list_id: ListId,
		item: ItemId,
		priority: Self::Priority,
		hint: Position<ItemId>,
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
	/// This is the LIFO primitive for consumers that insert equal-priority items
	/// and consume from the tail.
	///
	/// # Errors
	///
	/// - `CorruptList` if the tail pointer or list metadata is inconsistent.
	fn pop_tail(list_id: &ListId) -> Result<Option<(ItemId, Self::Priority)>, Self::Error>;

	/// Re-insert `(list_id, item)` at `new_priority`. Updates the priority in place
	/// when the existing neighbors still admit it; otherwise splices the item
	/// out and re-inserts at the hint. The returned [`Outcome`] tells the
	/// caller which path ran so the matching weight can be charged.
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
		new_priority: Self::Priority,
		hint: Position<ItemId>,
	) -> Result<Outcome, Self::Error>;

	/// Highest-priority item in `list_id`, or `None` if empty.
	fn head(list_id: &ListId) -> Option<ItemId>;

	/// Lowest-priority item in `list_id`, or `None` if empty.
	fn tail(list_id: &ListId) -> Option<ItemId>;

	/// Number of items in `list_id`.
	fn count(list_id: &ListId) -> u32;

	/// Returns `true` if `(list_id, item)` is in the list.
	fn contains(list_id: &ListId, item: &ItemId) -> bool;

	/// Current `(prev, next)` neighbors of `(list_id, item)`, if present.
	fn neighbors(list_id: &ListId, item: &ItemId) -> Option<Position<ItemId>>;

	/// Stored priority cached on `(list_id, item)`'s node, or `None` if absent.
	fn priority(list_id: &ListId, item: &ItemId) -> Option<Self::Priority>;

	/// First `n` items of `list_id` walking from the tail. Returns fewer than
	/// `n` if the list has fewer items.
	fn iter_from_tail(list_id: &ListId, n: u32) -> Vec<ItemId>;

	/// Insertion position for `priority` in `list_id`. O(list size); intended
	/// for hint preparation, not hot paths.
	fn find_position(list_id: &ListId, priority: Self::Priority) -> Position<ItemId>;

	/// Position `(list_id, item)` should occupy at `new_priority`, skipping the
	/// item's own node.
	///
	/// Returns `None` if the item is not in the list. O(list size); intended
	/// for hint preparation, not hot paths.
	fn find_re_insert_position(
		list_id: &ListId,
		item: &ItemId,
		new_priority: Self::Priority,
	) -> Option<Position<ItemId>>;

	/// Steps needed to repair `hint` for `priority` in `list_id`.
	///
	/// Returns `0` if the hint is already valid, or a value greater than
	/// `MaxHintRepairSteps` if the same dispatch would fail.
	fn repair_steps_needed(
		list_id: &ListId,
		priority: Self::Priority,
		hint: Position<ItemId>,
	) -> u32;
}

impl<T: Config> SortedListInterface<T::ListId, T::ItemId> for Pallet<T> {
	type Priority = T::Priority;
	type Error = Error<T>;

	fn insert(
		list_id: T::ListId,
		item: T::ItemId,
		priority: T::Priority,
		hint: Position<T::ItemId>,
	) -> Result<u32, Error<T>> {
		if ListNodes::<T>::contains_key(&list_id, &item) {
			return Err(Error::<T>::ItemAlreadyExists);
		}
		let (position, steps) = list::walk_repair::<T>(&list_id, &priority, hint)?;
		list::insert_at::<T>(&list_id, &item, priority, position)?;
		Self::deposit_event(Event::ItemInserted { list_id, item, priority });
		Ok(steps)
	}

	fn remove(list_id: &T::ListId, item: &T::ItemId) -> Result<(), Error<T>> {
		list::remove_at::<T>(list_id, item)?;
		Self::deposit_event(Event::ItemRemoved { list_id: list_id.clone(), item: item.clone() });
		Ok(())
	}

	fn pop_tail(list_id: &T::ListId) -> Result<Option<(T::ItemId, T::Priority)>, Error<T>> {
		let Some(item) = ListTails::<T>::get(list_id) else { return Ok(None) };
		let priority = ListNodes::<T>::get(list_id, &item)
			.defensive_ok_or(Error::<T>::CorruptList)?
			.priority;
		list::remove_at::<T>(list_id, &item)?;
		Self::deposit_event(Event::ItemRemoved { list_id: list_id.clone(), item: item.clone() });
		Ok(Some((item, priority)))
	}

	fn re_insert(
		list_id: T::ListId,
		item: T::ItemId,
		new_priority: T::Priority,
		hint: Position<T::ItemId>,
	) -> Result<Outcome, Error<T>> {
		let existing = ListNodes::<T>::get(&list_id, &item).ok_or(Error::<T>::ItemNotFound)?;
		let old_priority = existing.priority;

		// Fast path: same priority. No write, no event.
		if old_priority == new_priority {
			return Ok(Outcome::NoOp);
		}

		// Fast path: existing neighbors still admit the new priority, mutate in place.
		let existing_position = Position { prev: existing.prev, next: existing.next };
		if list::neighbor_priorities_admit::<T>(&list_id, &new_priority, &existing_position) {
			ListNodes::<T>::mutate(&list_id, &item, |maybe| {
				if let Some(n) = maybe {
					n.priority = new_priority;
				}
			});
			Self::deposit_event(Event::ItemReinserted {
				list_id,
				item,
				old_priority,
				new_priority,
			});
			return Ok(Outcome::InPlace);
		}

		// Slow path: splice + re-insert. Wrapped in a nested storage layer so
		// that an `InvalidPositionHints` after `remove_at` rolls back cleanly.
		let outer = with_transaction_opaque_err::<u32, Error<T>, _>(|| {
			let inner = (|| -> Result<u32, Error<T>> {
				list::remove_at::<T>(&list_id, &item)?;
				let (position, steps) = list::walk_repair::<T>(&list_id, &new_priority, hint)?;
				list::insert_at::<T>(&list_id, &item, new_priority, position)?;
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
		Self::deposit_event(Event::ItemReinserted { list_id, item, old_priority, new_priority });
		Ok(Outcome::Relocated { steps })
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

	fn neighbors(list_id: &T::ListId, item: &T::ItemId) -> Option<Position<T::ItemId>> {
		ListNodes::<T>::get(list_id, item).map(|n| Position { prev: n.prev, next: n.next })
	}

	fn priority(list_id: &T::ListId, item: &T::ItemId) -> Option<T::Priority> {
		ListNodes::<T>::get(list_id, item).map(|n| n.priority)
	}

	fn iter_from_tail(list_id: &T::ListId, n: u32) -> Vec<T::ItemId> {
		view_helpers::iter_from_tail::<T>(list_id, n)
	}

	fn find_position(list_id: &T::ListId, priority: T::Priority) -> Position<T::ItemId> {
		view_helpers::find_position::<T>(list_id, priority)
	}

	fn find_re_insert_position(
		list_id: &T::ListId,
		item: &T::ItemId,
		new_priority: T::Priority,
	) -> Option<Position<T::ItemId>> {
		view_helpers::find_re_insert_position::<T>(list_id, item, new_priority)
	}

	fn repair_steps_needed(
		list_id: &T::ListId,
		priority: T::Priority,
		hint: Position<T::ItemId>,
	) -> u32 {
		view_helpers::repair_steps_needed::<T>(list_id, priority, hint)
	}
}
