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

use crate::{list, pallet::*, view_helpers, ListError, Outcome, Position};
use alloc::vec::Vec;
use frame::deps::frame_support::{
	defensive,
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::Get,
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

	/// For benchmarks (and `std` test fixtures): pin the authoritative priority
	/// returned by [`Self::priority`] for `(list_id, item)`.
	#[cfg(any(feature = "runtime-benchmarks", feature = "std"))]
	fn set_priority(_list_id: &ListId, _item: &ItemId, _priority: Self::Priority) {}
}

/// Mutation and query surface for consumer pallets.
///
/// Position hints are [`Position`] values; endpoints are encoded as `None` in
/// each field. Mutating methods fail with [`ListError`] and return the number
/// of hint-repair steps actually walked so callers can refund unused weight
/// via `PostDispatchInfo::actual_weight`.
pub trait SortedListInterface<ListId, ItemId> {
	/// Priority type used to order items within a list.
	type Priority;

	/// Insert `(list_id, item)` at `priority`, repairing stale hints if needed.
	///
	/// # Errors
	///
	/// - [`ListError::ItemAlreadyExists`] if `(list_id, item)` is already in the list.
	/// - [`ListError::ListTooLong`] if the list's size counter would overflow.
	/// - [`ListError::InvalidPositionHints`] if the hint cannot be repaired within the budget.
	/// - [`ListError::CorruptList`] if a hinted neighbor row is missing or its links are
	///   inconsistent.
	fn insert(
		list_id: ListId,
		item: ItemId,
		priority: Self::Priority,
		hint: Position<ItemId>,
	) -> Result<u32, ListError>;

	/// Remove `(list_id, item)`.
	///
	/// # Errors
	///
	/// - [`ListError::ItemNotFound`] if `(list_id, item)` is not in the list.
	/// - [`ListError::CorruptList`] if the node exists but list metadata is inconsistent.
	fn remove(list_id: &ListId, item: &ItemId) -> Result<(), ListError>;

	/// Remove and return the current tail item of `list_id`, or `None` if the
	/// list is empty.
	///
	/// This is the LIFO primitive for consumers that insert equal-priority items
	/// and consume from the tail.
	///
	/// # Errors
	///
	/// - [`ListError::CorruptList`] if the tail pointer or list metadata is inconsistent.
	fn pop_tail(list_id: &ListId) -> Result<Option<(ItemId, Self::Priority)>, ListError>;

	/// Re-insert `(list_id, item)` at `new_priority`. Updates the priority in place
	/// when the existing neighbors still admit it; otherwise splices the item
	/// out and re-inserts at the hint. The returned [`Outcome`] tells the
	/// caller which path ran so the matching weight can be charged.
	///
	/// # Errors
	///
	/// - [`ListError::ItemNotFound`] if `(list_id, item)` is not in the list.
	/// - [`ListError::CorruptList`] if the node exists but list metadata is inconsistent.
	/// - [`ListError::InvalidPositionHints`] if the hint cannot be repaired within the budget.
	fn re_insert(
		list_id: ListId,
		item: ItemId,
		new_priority: Self::Priority,
		hint: Position<ItemId>,
	) -> Result<Outcome, ListError>;

	/// Highest-priority item in `list_id`, or `None` if empty.
	fn head(list_id: &ListId) -> Option<ItemId>;

	/// Lowest-priority item in `list_id`, or `None` if empty.
	fn tail(list_id: &ListId) -> Option<ItemId>;

	/// Number of items in `list_id`.
	fn count(list_id: &ListId) -> u32;

	/// Returns `true` if `(list_id, item)` is in the list.
	fn contains(list_id: &ListId, item: &ItemId) -> bool;

	/// Current `(prev, next)` neighbors of `(list_id, item)`, if present.
	fn neighbors(list_id: &ListId, item: &ItemId) -> Option<Position<ItemId>> {
		Self::node(list_id, item).map(|(_, position)| position)
	}

	/// Stored priority cached on `(list_id, item)`'s node, or `None` if absent.
	fn priority(list_id: &ListId, item: &ItemId) -> Option<Self::Priority> {
		Self::node(list_id, item).map(|(priority, _)| priority)
	}

	/// Stored priority and `(prev, next)` neighbors of `(list_id, item)` in a
	/// single read, or `None` if absent. The primitive behind [`Self::priority`]
	/// and [`Self::neighbors`]; prefer it when walking the list.
	fn node(list_id: &ListId, item: &ItemId) -> Option<(Self::Priority, Position<ItemId>)>;

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

	/// Maximum hint-repair walk length the implementation will accept before
	/// returning [`ListError::InvalidPositionHints`].
	fn repair_budget() -> u32;
}

impl<T: Config> SortedListInterface<T::ListId, T::ItemId> for Pallet<T> {
	type Priority = T::Priority;

	fn insert(
		list_id: T::ListId,
		item: T::ItemId,
		priority: T::Priority,
		hint: Position<T::ItemId>,
	) -> Result<u32, ListError> {
		if ListNodes::<T>::contains_key(&list_id, &item) {
			return Err(ListError::ItemAlreadyExists);
		}
		let (position, steps) = list::walk_repair::<T>(&list_id, &priority, hint)?;
		let list_created = list::insert_at::<T>(&list_id, &item, priority, position)?;
		if list_created {
			Self::deposit_event(Event::ListCreated { list_id: list_id.clone() });
		}
		Self::deposit_event(Event::ItemInserted { list_id, item, priority });
		Ok(steps)
	}

	fn remove(list_id: &T::ListId, item: &T::ItemId) -> Result<(), ListError> {
		let (priority, list_removed) = list::remove_at::<T>(list_id, item)?;
		Self::deposit_event(Event::ItemRemoved {
			list_id: list_id.clone(),
			item: item.clone(),
			priority,
		});
		if list_removed {
			Self::deposit_event(Event::ListRemoved { list_id: list_id.clone() });
		}
		Ok(())
	}

	fn pop_tail(list_id: &T::ListId) -> Result<Option<(T::ItemId, T::Priority)>, ListError> {
		let Some(item) = ListMetas::<T>::get(list_id).and_then(|m| m.tail) else { return Ok(None) };
		let (priority, list_removed) =
			list::remove_at::<T>(list_id, &item).map_err(|e| match e {
				// The item id came from the meta row, so a missing node row is
				// internal inconsistency, not a caller error.
				ListError::ItemNotFound => {
					defensive!("pop_tail: tail pointer names a missing node");
					ListError::CorruptList
				},
				other => other,
			})?;
		Self::deposit_event(Event::ItemRemoved {
			list_id: list_id.clone(),
			item: item.clone(),
			priority,
		});
		if list_removed {
			Self::deposit_event(Event::ListRemoved { list_id: list_id.clone() });
		}
		Ok(Some((item, priority)))
	}

	fn re_insert(
		list_id: T::ListId,
		item: T::ItemId,
		new_priority: T::Priority,
		hint: Position<T::ItemId>,
	) -> Result<Outcome, ListError> {
		let existing = ListNodes::<T>::get(&list_id, &item).ok_or(ListError::ItemNotFound)?;
		let old_priority = existing.priority;

		// Fast path: same priority. No write, no event.
		if old_priority == new_priority {
			return Ok(Outcome::NoOp);
		}

		// Fast path: existing neighbors still admit the new priority, mutate in place.
		let existing_position = existing.into_position();
		let (prev_node, next_node) = list::neighbor_nodes::<T>(&list_id, &existing_position);
		if list::neighbor_priorities_admit(
			&new_priority,
			&existing_position,
			prev_node.as_ref(),
			next_node.as_ref(),
		) {
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
		let outer = with_transaction_opaque_err::<u32, ListError, _>(|| {
			let inner = (|| -> Result<u32, ListError> {
				// The item never leaves the list, so the lifecycle flags from
				// `remove_at`/`insert_at` are intentionally dropped — emitting
				// `ListRemoved`/`ListCreated` here would churn a single-item relocate.
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
		let steps = outer.map_err(|()| ListError::InvalidPositionHints)??;
		Self::deposit_event(Event::ItemReinserted { list_id, item, old_priority, new_priority });
		Ok(Outcome::Relocated { steps })
	}

	fn head(list_id: &T::ListId) -> Option<T::ItemId> {
		ListMetas::<T>::get(list_id).and_then(|m| m.head)
	}

	fn tail(list_id: &T::ListId) -> Option<T::ItemId> {
		ListMetas::<T>::get(list_id).and_then(|m| m.tail)
	}

	fn count(list_id: &T::ListId) -> u32 {
		ListMetas::<T>::get(list_id).map_or(0, |m| m.len)
	}

	fn contains(list_id: &T::ListId, item: &T::ItemId) -> bool {
		ListNodes::<T>::contains_key(list_id, item)
	}

	fn node(list_id: &T::ListId, item: &T::ItemId) -> Option<(T::Priority, Position<T::ItemId>)> {
		ListNodes::<T>::get(list_id, item).map(|n| (n.priority, n.into_position()))
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

	fn repair_budget() -> u32 {
		T::MaxHintRepairSteps::get()
	}
}
