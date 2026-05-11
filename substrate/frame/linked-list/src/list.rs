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
//! [`walk_repair`] mutate or read the per-list [`ListNodes`], [`ListHeads`],
//! [`ListTails`] and [`ListSizes`] storage maps and are wrapped by the trait
//! impl in [`super::sorted_list_interface`].

use crate::{pallet::*, Position};
use frame::{deps::frame_support::traits::DefensiveOption, prelude::*};

/// Fetch a neighbor's stored node, returning `CorruptList` if `id` is `Some` but
/// no row exists. Used by [`insert_at`] and [`remove_at`] to surface dangling
/// links rather than silently no-op-mutating.
fn fetch_neighbor<T: Config>(
	list_id: &T::ListId,
	id: Option<T::ItemId>,
) -> Result<Option<(T::ItemId, Node<T::ItemId, T::Priority>)>, Error<T>> {
	id.map(|i| ListNodes::<T>::get(list_id, &i).ok_or(Error::<T>::CorruptList).map(|n| (i, n)))
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

/// Whether `pos` is a valid insert position for `priority` in `list_id`.
///
/// Checks that the link structure is consistent with the list's head/tail
/// pointers and that `prev.priority >= priority > next.priority` (with endpoints
/// treated as `+inf` / `-inf`). The `>=`/`>` asymmetry places same-priority
/// inserts on the tail side of their cluster, yielding LIFO under tail-first
/// iteration.
pub fn is_position_valid<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	pos: &Position<T::ItemId>,
) -> bool {
	let prev_ok = pos.prev.as_ref().map_or_else(
		|| ListHeads::<T>::get(list_id) == pos.next,
		|p| {
			ListNodes::<T>::get(list_id, p)
				.is_some_and(|n| n.next == pos.next && n.priority >= *priority)
		},
	);
	if !prev_ok {
		return false;
	}

	pos.next.as_ref().map_or_else(
		|| ListTails::<T>::get(list_id) == pos.prev,
		|n| {
			ListNodes::<T>::get(list_id, n)
				.is_some_and(|node| node.prev == pos.prev && *priority > node.priority)
		},
	)
}

/// Priority-only half of [`is_position_valid`]. Skips the link-consistency check
/// and is used by `re_insert`'s in-place fast path, where the existing links
/// are valid by construction.
pub fn neighbor_priorities_admit<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	pos: &Position<T::ItemId>,
) -> bool {
	let prev_ok = pos.prev.as_ref().is_none_or(|p| {
		ListNodes::<T>::get(list_id, p).is_some_and(|n| n.priority >= *priority)
	});
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

/// If `current` references a removed node, clear that side. Tries `prev` then
/// `next`; clears at most one side per call so each clamp counts as a distinct
/// repair step.
fn try_clamp_dangling<T: Config>(list_id: &T::ListId, current: &mut Position<T::ItemId>) -> bool {
	if let Some(p) = current.prev.as_ref() {
		if !ListNodes::<T>::contains_key(list_id, p) {
			current.prev = None;
			return true;
		}
	}
	if let Some(n) = current.next.as_ref() {
		if !ListNodes::<T>::contains_key(list_id, n) {
			current.next = None;
			return true;
		}
	}
	false
}

/// If `current.prev` and `current.next` are not adjacent via their cached
/// links, re-anchor on whichever side's cached link admits `priority`
/// (preferring `prev` on ties). Returns `false` when the links are already
/// consistent.
fn try_reanchor_inconsistent<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	current: &mut Position<T::ItemId>,
	prev_node: Option<&Node<T::ItemId, T::Priority>>,
	next_node: Option<&Node<T::ItemId, T::Priority>>,
) -> bool {
	let prev_links_match = prev_node
		.map_or_else(|| ListHeads::<T>::get(list_id) == current.next, |pn| pn.next == current.next);
	let next_links_match = next_node
		.map_or_else(|| ListTails::<T>::get(list_id) == current.prev, |nn| nn.prev == current.prev);

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
			current.next = ListHeads::<T>::get(list_id);
		},
	}
	true
}

/// Walk one node head-ward (if `prev.priority < priority`) or tail-ward (if
/// `priority <= next.priority`). The `<=` preserves the `>=`/`>` asymmetry of
/// [`is_position_valid`]. Returns `false` when neither side admits a walk —
/// only reachable when links are consistent yet the position is rejected,
/// which the caller treats as a contract violation.
fn try_walk_priority<T: Config>(
	priority: &T::Priority,
	current: &mut Position<T::ItemId>,
	prev_node: Option<&Node<T::ItemId, T::Priority>>,
	next_node: Option<&Node<T::ItemId, T::Priority>>,
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
/// at most `MaxHintRepairSteps` steps.
///
/// Returns the corrected position alongside the number of steps actually
/// taken, or `InvalidPositionHints` if the budget is exhausted before a valid
/// position is reached.
pub fn walk_repair<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	hint: Position<T::ItemId>,
) -> Result<(Position<T::ItemId>, u32), Error<T>> {
	let mut current = hint;
	if is_position_valid::<T>(list_id, priority, &current) {
		return Ok((current, 0));
	}

	let budget = T::MaxHintRepairSteps::get();
	for steps in 1..=budget {
		let progressed = try_clamp_dangling::<T>(list_id, &mut current) || {
			let prev_node = current.prev.as_ref().and_then(|p| ListNodes::<T>::get(list_id, p));
			let next_node = current.next.as_ref().and_then(|n| ListNodes::<T>::get(list_id, n));
			let (pn, nn) = (prev_node.as_ref(), next_node.as_ref());
			try_reanchor_inconsistent::<T>(list_id, priority, &mut current, pn, nn) ||
				try_walk_priority::<T>(priority, &mut current, pn, nn)
		};
		if !progressed {
			// Links are consistent and neither side's priority drives a walk,
			// yet `is_position_valid` rejects us. Reset to the head so the
			// loop still terminates within the budget.
			defensive!("walk_repair: no repair step applicable, resetting to head");
			current.prev = None;
			current.next = ListHeads::<T>::get(list_id);
		}
		if is_position_valid::<T>(list_id, priority, &current) {
			return Ok((current, steps));
		}
	}

	crate::log!(debug, "walk_repair: stale hint exceeded MaxHintRepairSteps ({} steps)", budget,);
	Err(Error::<T>::InvalidPositionHints)
}

/// Insert `item` at `position` in `list_id`. The caller is responsible for
/// ensuring the position is valid; errors if `item` is already in the list.
pub fn insert_at<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	priority: T::Priority,
	position: Position<T::ItemId>,
) -> Result<(), Error<T>> {
	if ListNodes::<T>::contains_key(list_id, item) {
		return Err(Error::<T>::ItemAlreadyExists);
	}
	let new_size = ListSizes::<T>::get(list_id).checked_add(1).ok_or(Error::<T>::ListTooLong)?;

	let prev_node = fetch_neighbor::<T>(list_id, position.prev.clone())?;
	let next_node = fetch_neighbor::<T>(list_id, position.next.clone())?;

	if prev_node.as_ref().is_some_and(|(_, node)| node.next != position.next) ||
		next_node.as_ref().is_some_and(|(_, node)| node.prev != position.prev)
	{
		return Err(Error::<T>::CorruptList);
	}

	// This reads storage and is intentionally debug-only; dispatch weights are
	// benchmarked without `debug_assertions`.
	debug_assert!(is_position_valid::<T>(list_id, &priority, &position));

	// Splice in on the head side: rewrite prev's `.next` to point at `item`,
	// or promote `item` to head if there's no prev.
	match prev_node {
		Some((p, mut n)) => {
			n.next = Some(item.clone());
			ListNodes::<T>::insert(list_id, p, n);
		},
		None => ListHeads::<T>::insert(list_id, item),
	}
	// Symmetric on the tail side.
	match next_node {
		Some((n, mut node)) => {
			node.prev = Some(item.clone());
			ListNodes::<T>::insert(list_id, n, node);
		},
		None => ListTails::<T>::insert(list_id, item),
	}

	ListNodes::<T>::insert(
		list_id,
		item,
		Node { prev: position.prev, next: position.next, priority },
	);
	ListSizes::<T>::insert(list_id, new_size);
	Ok(())
}

/// Remove `item` from `list_id`. Cleans up the list's
/// `ListHeads`/`ListTails`/`ListSizes` rows when it becomes empty. Errors if
/// `item` is not in the list.
pub fn remove_at<T: Config>(list_id: &T::ListId, item: &T::ItemId) -> Result<(), Error<T>> {
	let Node { prev: removed_prev, next: removed_next, .. } =
		ListNodes::<T>::get(list_id, item).ok_or(Error::<T>::ItemNotFound)?;
	// Defensive: by `try_state` invariant 1, a present node implies `ListSizes >= 1`.
	let new_size = ListSizes::<T>::get(list_id)
		.checked_sub(1)
		.defensive_ok_or(Error::<T>::CorruptList)?;
	if removed_prev.as_ref() == Some(item) || removed_next.as_ref() == Some(item) {
		return Err(Error::<T>::CorruptList);
	}

	let prev_node = fetch_neighbor::<T>(list_id, removed_prev.clone())?;
	let next_node = fetch_neighbor::<T>(list_id, removed_next.clone())?;

	if prev_node.as_ref().is_some_and(|(_, node)| node.next.as_ref() != Some(item)) ||
		next_node.as_ref().is_some_and(|(_, node)| node.prev.as_ref() != Some(item))
	{
		return Err(Error::<T>::CorruptList);
	}

	ListNodes::<T>::remove(list_id, item);

	// Splice past `item` on the head side: rewrite prev's `.next` to skip us,
	// or rewrite the head pointer to `removed_next` (which clears it when the
	// list emptied).
	match prev_node {
		Some((p, mut left)) => {
			left.next = removed_next;
			ListNodes::<T>::insert(list_id, p, left);
		},
		None => match removed_next {
			Some(new_head) => ListHeads::<T>::insert(list_id, new_head),
			None => ListHeads::<T>::remove(list_id),
		},
	}
	// Symmetric on the tail side.
	match next_node {
		Some((n, mut right)) => {
			right.prev = removed_prev;
			ListNodes::<T>::insert(list_id, n, right);
		},
		None => match removed_prev {
			Some(new_tail) => ListTails::<T>::insert(list_id, new_tail),
			None => ListTails::<T>::remove(list_id),
		},
	}

	if new_size == 0 {
		ListSizes::<T>::remove(list_id);
	} else {
		ListSizes::<T>::insert(list_id, new_size);
	}
	Ok(())
}
