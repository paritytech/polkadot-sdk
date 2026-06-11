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

//! Storage primitives for the sorted doubly-linked list.
//!
//! [`Node`] is the per-item storage value. [`insert_at`], [`remove_at`] and
//! [`walk_repair`] mutate or read the per-list [`ListNodes`] and [`ListMetas`]
//! storage maps and are wrapped by the trait impl in
//! [`super::sorted_list_interface`].

use crate::{pallet::*, ListError, ListMeta, Position};
use frame::{deps::frame_support::traits::DefensiveOption, prelude::*};

/// Fetch a neighbor's stored node. A `Some(id)` that resolves to no row is
/// internal corruption — a healthy node never links to an absent neighbor.
fn fetch_neighbor<T: Config>(
	list_id: &T::ListId,
	id: Option<T::ItemId>,
) -> Result<Option<(T::ItemId, Node<T::ItemId, T::Priority>)>, ListError> {
	id.map(|i| {
		ListNodes::<T>::get(list_id, &i)
			.defensive_ok_or(ListError::CorruptList)
			.map(|n| (i, n))
	})
	.transpose()
}

/// One node of a per-list sorted list.
///
/// `prev`/`next` are `None` at the head/tail endpoints. The priority is cached
/// alongside the links so that position checks do not require a read into the
/// consumer's source of truth.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct Node<ItemId, Priority> {
	pub prev: Option<ItemId>,
	pub next: Option<ItemId>,
	pub priority: Priority,
}

impl<ItemId, Priority> Node<ItemId, Priority> {
	/// Consume the node into its `(prev, next)` [`Position`].
	pub fn into_position(self) -> Position<ItemId> {
		Position { prev: self.prev, next: self.next }
	}
}

/// Whether `pos` is a valid insert position for `priority`.
///
/// Valid means the neighbor links are consistent with the gap (endpoints
/// checked against `meta`) and `prev.priority >= priority > next.priority`,
/// with endpoints treated as `+inf` / `-inf`. The `>=`/`>` asymmetry places
/// same-priority inserts on the tail side of their cluster, yielding LIFO
/// under tail-first iteration.
///
/// `prev_node`/`next_node` are the stored nodes for `pos.prev`/`pos.next`,
/// `None` for an endpoint or a missing row.
pub fn is_position_valid<ItemId: PartialEq, Priority: Ord>(
	priority: &Priority,
	pos: &Position<ItemId>,
	prev_node: Option<&Node<ItemId, Priority>>,
	next_node: Option<&Node<ItemId, Priority>>,
	meta: Option<&ListMeta<ItemId>>,
) -> bool {
	let prev_ok = match (&pos.prev, prev_node) {
		(None, _) => meta.and_then(|m| m.head.as_ref()) == pos.next.as_ref(),
		(Some(_), Some(node)) => node.next == pos.next && node.priority >= *priority,
		(Some(_), None) => false,
	};
	if !prev_ok {
		return false;
	}

	match (&pos.next, next_node) {
		(None, _) => meta.and_then(|m| m.tail.as_ref()) == pos.prev.as_ref(),
		(Some(_), Some(node)) => node.prev == pos.prev && *priority > node.priority,
		(Some(_), None) => false,
	}
}

/// Priority-only half of [`is_position_valid`]. Skips the link-consistency check
/// and is used by `re_insert`'s in-place fast path, where the existing links
/// are valid by construction.
pub fn neighbor_priorities_admit<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	pos: &Position<T::ItemId>,
) -> bool {
	let prev_ok = pos
		.prev
		.as_ref()
		.is_none_or(|p| ListNodes::<T>::get(list_id, p).is_some_and(|n| n.priority >= *priority));
	let next_ok = pos.next.as_ref().is_none_or(|n| {
		ListNodes::<T>::get(list_id, n).is_some_and(|node| *priority > node.priority)
	});
	prev_ok && next_ok
}

// Each `try_*` returns `true` iff it applied a one-step mutation to `current`.
// [`walk_repair`] dispatches them in order — clamp dangling refs, then
// re-anchor inconsistent links, then walk by priority — and treats all three
// returning `false` as a contract violation (the cursor is rejected by
// [`is_position_valid`] yet no repair step applies).

/// If `current` references a removed node, clear that side. Clears at most
/// one side per call so each clamp counts as a distinct repair step.
fn try_clamp_dangling<ItemId, Priority>(
	current: &mut Position<ItemId>,
	prev_node: Option<&Node<ItemId, Priority>>,
	next_node: Option<&Node<ItemId, Priority>>,
) -> bool {
	if current.prev.is_some() && prev_node.is_none() {
		current.prev = None;
		return true;
	}
	if current.next.is_some() && next_node.is_none() {
		current.next = None;
		return true;
	}
	false
}

/// If `current.prev` and `current.next` are not adjacent via their cached
/// links, re-anchor on whichever side's cached link admits `priority`
/// (preferring `prev` on ties). Returns `false` when the links are already
/// consistent.
fn try_reanchor_inconsistent<ItemId: Clone + PartialEq, Priority: Ord>(
	priority: &Priority,
	current: &mut Position<ItemId>,
	prev_node: Option<&Node<ItemId, Priority>>,
	next_node: Option<&Node<ItemId, Priority>>,
	meta: Option<&ListMeta<ItemId>>,
) -> bool {
	let prev_links_match = prev_node.map_or_else(
		|| meta.and_then(|m| m.head.as_ref()) == current.next.as_ref(),
		|pn| pn.next == current.next,
	);
	let next_links_match = next_node.map_or_else(
		|| meta.and_then(|m| m.tail.as_ref()) == current.prev.as_ref(),
		|nn| nn.prev == current.prev,
	);

	if prev_links_match && next_links_match {
		return false;
	}

	match (prev_node, next_node) {
		(Some(pn), Some(nn)) => {
			if pn.priority >= *priority {
				current.next = pn.next.clone();
			} else if *priority > nn.priority {
				current.prev = nn.prev.clone();
			} else {
				current.next = pn.next.clone();
			}
		},
		(Some(pn), None) => current.next = pn.next.clone(),
		(None, Some(nn)) => current.prev = nn.prev.clone(),
		(None, None) => {
			current.prev = None;
			current.next = meta.and_then(|m| m.head.clone());
		},
	}
	true
}

/// Walk one node head-ward (if `prev.priority < priority`) or tail-ward (if
/// `priority <= next.priority`). The `<=` preserves the `>=`/`>` asymmetry of
/// [`is_position_valid`]. Returns `false` when neither side admits a walk —
/// only reachable when links are consistent yet the position is rejected,
/// which the caller treats as a contract violation.
fn try_walk_priority<ItemId: Clone, Priority: Ord>(
	priority: &Priority,
	current: &mut Position<ItemId>,
	prev_node: Option<&Node<ItemId, Priority>>,
	next_node: Option<&Node<ItemId, Priority>>,
) -> bool {
	if let Some(pn) = prev_node.filter(|n| n.priority < *priority) {
		current.next = current.prev.take();
		current.prev = pn.prev.clone();
		true
	} else if let Some(nn) = next_node.filter(|n| *priority <= n.priority) {
		current.prev = current.next.take();
		current.next = nn.next.clone();
		true
	} else {
		false
	}
}

/// Walk from `hint` toward the correct insert position for `priority`, taking
/// at most `MaxHintRepairSteps` steps. Each iteration fetches the two
/// neighbor rows once; the decoded pair feeds both the validity check and
/// the repair step.
///
/// Returns the corrected position alongside the number of steps actually
/// taken, or `InvalidPositionHints` if the budget is exhausted before a valid
/// position is reached.
pub fn walk_repair<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	hint: Position<T::ItemId>,
) -> Result<(Position<T::ItemId>, u32), ListError> {
	let meta = ListMetas::<T>::get(list_id);
	let mut current = hint;
	let budget = T::MaxHintRepairSteps::get();
	// `steps` counts mutations applied so far; the range is inclusive so the
	// `budget`-th mutation still gets its validation pass.
	for steps in 0..=budget {
		let prev_node = current.prev.as_ref().and_then(|p| ListNodes::<T>::get(list_id, p));
		let next_node = current.next.as_ref().and_then(|n| ListNodes::<T>::get(list_id, n));
		let (pn, nn) = (prev_node.as_ref(), next_node.as_ref());
		if is_position_valid(priority, &current, pn, nn, meta.as_ref()) {
			return Ok((current, steps));
		}
		if steps == budget {
			break;
		}
		let progressed = try_clamp_dangling(&mut current, pn, nn) ||
			try_reanchor_inconsistent(priority, &mut current, pn, nn, meta.as_ref()) ||
			try_walk_priority(priority, &mut current, pn, nn);
		if !progressed {
			// Links are consistent and neither side's priority drives a walk,
			// yet `is_position_valid` rejects us. Reset to the head so the
			// loop still terminates within the budget.
			defensive!("walk_repair: no repair step applicable, resetting to head");
			current.prev = None;
			current.next = meta.as_ref().and_then(|m| m.head.clone());
		}
	}

	crate::log!(debug, "walk_repair: stale hint exceeded MaxHintRepairSteps ({} steps)", budget,);
	Err(ListError::InvalidPositionHints)
}

/// Post-validate the resolved insert position: each present neighbor's cached
/// link must point across the gap and its priority must satisfy
/// `prev.priority >= p > next.priority`.
fn assert_position_admits<T: Config>(
	priority: &T::Priority,
	position: &Position<T::ItemId>,
	prev_node: Option<&Node<T::ItemId, T::Priority>>,
	next_node: Option<&Node<T::ItemId, T::Priority>>,
) -> Result<(), ListError> {
	if let Some(prev) = prev_node {
		if prev.next != position.next || prev.priority < *priority {
			defensive!("insert_at: prev neighbor rejects the resolved position");
			return Err(ListError::CorruptList);
		}
	}
	if let Some(next) = next_node {
		if next.prev != position.prev || *priority <= next.priority {
			defensive!("insert_at: next neighbor rejects the resolved position");
			return Err(ListError::CorruptList);
		}
	}
	Ok(())
}

/// Apply the head/tail/len bookkeeping for inserting `item` at `position`,
/// folding the endpoint cross-check and overflow guard into one `ListMetas` row
/// write. On any error the row is left untouched. A `None`-side hint inserts at
/// the head/tail, so the existing head/tail pointer must agree with the other
/// side.
///
/// Returns `true` if this insert created the list (the `ListMetas` row was absent).
fn update_meta_for_insert<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	position: &Position<T::ItemId>,
) -> Result<bool, ListError> {
	ListMetas::<T>::try_mutate_exists(list_id, |slot| -> Result<bool, ListError> {
		let list_created = slot.is_none();
		let mut meta = slot.take().unwrap_or_default();
		if position.prev.is_none() && meta.head != position.next {
			defensive!("insert_at: head pointer disagrees with head-side insert");
			return Err(ListError::CorruptList);
		}
		if position.next.is_none() && meta.tail != position.prev {
			defensive!("insert_at: tail pointer disagrees with tail-side insert");
			return Err(ListError::CorruptList);
		}
		// Caller-reachable capacity limit, not corruption: a list can legitimately
		// hold `u32::MAX` items, so overflow stays a graceful `ListTooLong` rather
		// than the `defensive!` posture of the surrounding consistency checks.
		meta.len = meta.len.checked_add(1).ok_or(ListError::ListTooLong)?;
		if position.prev.is_none() {
			meta.head = Some(item.clone());
		}
		if position.next.is_none() {
			meta.tail = Some(item.clone());
		}
		*slot = Some(meta);
		Ok(list_created)
	})
}

/// Insert `item` at `position` in `list_id`. The caller is responsible for
/// ensuring the position is valid; errors if `item` is already in the list.
///
/// Returns `true` if this insert created the list (it was previously empty).
pub fn insert_at<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	priority: T::Priority,
	position: Position<T::ItemId>,
) -> Result<bool, ListError> {
	if ListNodes::<T>::contains_key(list_id, item) {
		return Err(ListError::ItemAlreadyExists);
	}

	// Anti-cycle guard: a node must never be linked against itself. `walk_repair`
	// never yields such a position, so reaching it is internal corruption.
	if position.prev.as_ref() == Some(item) || position.next.as_ref() == Some(item) {
		defensive!("insert_at: item linked against itself");
		return Err(ListError::CorruptList);
	}

	let prev_node = fetch_neighbor::<T>(list_id, position.prev.clone())?;
	let next_node = fetch_neighbor::<T>(list_id, position.next.clone())?;

	assert_position_admits::<T>(
		&priority,
		&position,
		prev_node.as_ref().map(|(_, n)| n),
		next_node.as_ref().map(|(_, n)| n),
	)?;

	let list_created = update_meta_for_insert::<T>(list_id, item, &position)?;

	// Splice in on the head side: rewrite prev's `.next` to point at `item`.
	if let Some((p, mut n)) = prev_node {
		n.next = Some(item.clone());
		ListNodes::<T>::insert(list_id, p, n);
	}
	// Symmetric on the tail side.
	if let Some((n, mut node)) = next_node {
		node.prev = Some(item.clone());
		ListNodes::<T>::insert(list_id, n, node);
	}

	ListNodes::<T>::insert(
		list_id,
		item,
		Node { prev: position.prev, next: position.next, priority },
	);

	// Pair the pre-splice `assert_position_admits` check with an after-write read.
	#[cfg(debug_assertions)]
	debug_assert_insert_post_condition::<T>(list_id, item);

	Ok(list_created)
}

/// Debug-only post-condition for [`insert_at`]: re-read `item` and confirm the
/// splice landed — the cached priorities still admit `item`, each present
/// neighbor points back at it, and a `None` side owns the matching endpoint.
#[cfg(debug_assertions)]
fn debug_assert_insert_post_condition<T: Config>(list_id: &T::ListId, item: &T::ItemId) {
	let node = ListNodes::<T>::get(list_id, item).expect("insert_at just wrote item; qed");
	let position = Position { prev: node.prev, next: node.next };

	// Priority ordering: reuse the production priority-only predicate rather than
	// re-spelling the `>=`/`>` asymmetry.
	debug_assert!(
		neighbor_priorities_admit::<T>(list_id, &node.priority, &position),
		"neighbor priorities reject item",
	);

	match position.prev {
		Some(ref prev_id) => debug_assert_eq!(
			ListNodes::<T>::get(list_id, prev_id).and_then(|n| n.next).as_ref(),
			Some(item),
			"prev.next must point to item",
		),
		None => debug_assert_eq!(
			ListMetas::<T>::get(list_id).and_then(|m| m.head).as_ref(),
			Some(item),
			"head-side insert must own the head pointer",
		),
	}
	match position.next {
		Some(ref next_id) => debug_assert_eq!(
			ListNodes::<T>::get(list_id, next_id).and_then(|n| n.prev).as_ref(),
			Some(item),
			"next.prev must point to item",
		),
		None => debug_assert_eq!(
			ListMetas::<T>::get(list_id).and_then(|m| m.tail).as_ref(),
			Some(item),
			"tail-side insert must own the tail pointer",
		),
	}
}

/// Remove `item` from `list_id`. Drops the [`ListMetas`] row when the list
/// becomes empty. Errors if `item` is not in the list.
///
/// Returns the removed node's priority, plus `true` if this remove emptied
/// the list (the `ListMetas` row was dropped).
pub fn remove_at<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
) -> Result<(T::Priority, bool), ListError> {
	let Node { prev: removed_prev, next: removed_next, priority } =
		ListNodes::<T>::get(list_id, item).ok_or(ListError::ItemNotFound)?;
	// Anti-cycle guard: a node's own links must never name itself.
	if removed_prev.as_ref() == Some(item) || removed_next.as_ref() == Some(item) {
		defensive!("remove_at: node linked against itself");
		return Err(ListError::CorruptList);
	}

	let prev_node = fetch_neighbor::<T>(list_id, removed_prev.clone())?;
	let next_node = fetch_neighbor::<T>(list_id, removed_next.clone())?;

	if prev_node.as_ref().is_some_and(|(_, node)| node.next.as_ref() != Some(item)) {
		defensive!("remove_at: prev neighbor does not link back to item");
		return Err(ListError::CorruptList);
	}
	if next_node.as_ref().is_some_and(|(_, node)| node.prev.as_ref() != Some(item)) {
		defensive!("remove_at: next neighbor does not link back to item");
		return Err(ListError::CorruptList);
	}

	// Validate endpoints and update the meta row first so that any cross-check
	// failure surfaces as `CorruptList` before any node-row mutation happens.
	// The row vanishes once the list empties.
	let list_removed =
		ListMetas::<T>::try_mutate_exists(list_id, |slot| -> Result<bool, ListError> {
			// Defensive: by the all-or-nothing invariant a present node implies a
			// present meta row with `len >= 1`.
			let mut meta = slot.take().defensive_ok_or(ListError::CorruptList)?;
			// Endpoint cross-check: a `None`-side neighbor link means `item` must
			// be the stored head/tail; otherwise storage is internally inconsistent.
			if removed_prev.is_none() && meta.head.as_ref() != Some(item) {
				defensive!("remove_at: head pointer disagrees with removed head node");
				return Err(ListError::CorruptList);
			}
			if removed_next.is_none() && meta.tail.as_ref() != Some(item) {
				defensive!("remove_at: tail pointer disagrees with removed tail node");
				return Err(ListError::CorruptList);
			}
			meta.len = meta.len.checked_sub(1).defensive_ok_or(ListError::CorruptList)?;
			// We removed the head iff the removed node had no `prev`; the new head is
			// then the removed item's `next` (`None` once the list empties).
			if removed_prev.is_none() {
				meta.head = removed_next.clone();
			}
			// Symmetric for the tail.
			if removed_next.is_none() {
				meta.tail = removed_prev.clone();
			}
			let list_emptied = meta.len == 0;
			if !list_emptied {
				*slot = Some(meta);
			}
			Ok(list_emptied)
		})?;

	ListNodes::<T>::remove(list_id, item);

	// Splice past `item` in the neighbors' node rows.
	if let Some((p, mut left)) = prev_node {
		left.next = removed_next;
		ListNodes::<T>::insert(list_id, p, left);
	}
	if let Some((n, mut right)) = next_node {
		right.prev = removed_prev;
		ListNodes::<T>::insert(list_id, n, right);
	}

	Ok((priority, list_removed))
}
