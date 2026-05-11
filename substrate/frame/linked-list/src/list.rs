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

use crate::{pallet::*, Position, Side};
use frame::{deps::frame_support::traits::DefensiveOption, prelude::*};

/// Fetch a neighbor's stored node, returning `CorruptList` if `id` is `Some` but
/// no row exists. Used by [`insert_at`] and [`remove_at`] to surface dangling
/// links rather than silently no-op-mutating.
fn fetch_neighbor<T: Config>(
	list_id: &T::ListId,
	id: Option<T::ItemId>,
) -> Result<Option<(T::ItemId, Node<T::ItemId, T::Priority>)>, Error<T>> {
	id.map(|i| ListNodes::<T>::get(list_id, i).ok_or(Error::<T>::CorruptList).map(|n| (i, n)))
		.transpose()
}

/// Write or clear `ListHeads[list_id]` based on whether `item` is `Some`.
#[inline]
fn set_head<T: Config>(list_id: &T::ListId, item: Option<T::ItemId>) {
	match item {
		Some(i) => ListHeads::<T>::insert(list_id, i),
		None => ListHeads::<T>::remove(list_id),
	}
}

/// Write or clear `ListTails[list_id]` based on whether `item` is `Some`.
#[inline]
fn set_tail<T: Config>(list_id: &T::ListId, item: Option<T::ItemId>) {
	match item {
		Some(i) => ListTails::<T>::insert(list_id, i),
		None => ListTails::<T>::remove(list_id),
	}
}

/// Splice `inserted` in on the head side: if `prev_neighbor` is `Some`, point
/// its cached `.next` at `inserted` and write back; otherwise `inserted`
/// becomes the new list head.
///
/// Caller must have validated the position upfront; this only mutates.
#[inline]
fn link_prev_side<T: Config>(
	list_id: &T::ListId,
	prev_neighbor: Option<(T::ItemId, Node<T::ItemId, T::Priority>)>,
	inserted: T::ItemId,
) {
	match prev_neighbor {
		Some((p, mut node)) => {
			node.next = Some(inserted);
			ListNodes::<T>::insert(list_id, p, node);
		},
		None => ListHeads::<T>::insert(list_id, inserted),
	}
}

/// Symmetric: splice `inserted` in on the tail side.
#[inline]
fn link_next_side<T: Config>(
	list_id: &T::ListId,
	next_neighbor: Option<(T::ItemId, Node<T::ItemId, T::Priority>)>,
	inserted: T::ItemId,
) {
	match next_neighbor {
		Some((n, mut node)) => {
			node.prev = Some(inserted);
			ListNodes::<T>::insert(list_id, n, node);
		},
		None => ListTails::<T>::insert(list_id, inserted),
	}
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
	let prev_ok = pos.prev.map_or_else(
		|| ListHeads::<T>::get(list_id) == pos.next,
		|p| {
			ListNodes::<T>::get(list_id, p)
				.is_some_and(|n| n.next == pos.next && n.priority >= *priority)
		},
	);
	if !prev_ok {
		return false;
	}

	pos.next.map_or_else(
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
	let prev_ok = pos
		.prev
		.map_or(true, |p| ListNodes::<T>::get(list_id, p).is_some_and(|n| n.priority >= *priority));
	let next_ok = pos.next.map_or(true, |n| {
		ListNodes::<T>::get(list_id, n).is_some_and(|node| *priority > node.priority)
	});
	prev_ok && next_ok
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
	let Position { mut prev, mut next } = hint;
	// Helper closure: package the loop-local cursor as a `Position` for the
	// `is_position_valid` boundary, which the rest of the module expects.
	let valid = |p, n, prio| is_position_valid::<T>(list_id, prio, &Position { prev: p, next: n });
	if valid(prev, next, priority) {
		return Ok((Position { prev, next }, 0));
	}

	let budget = T::MaxHintRepairSteps::get();
	let mut steps = 0u32;

	while steps < budget {
		// Clamp dangling hints (caller referenced a removed node) to `None`.
		if let Some(p) = prev {
			if !ListNodes::<T>::contains_key(list_id, p) {
				prev = None;
				steps = steps.saturating_add(1);
				if valid(prev, next, priority) {
					return Ok((Position { prev, next }, steps));
				}
				continue;
			}
		}
		if let Some(n) = next {
			if !ListNodes::<T>::contains_key(list_id, n) {
				next = None;
				steps = steps.saturating_add(1);
				if valid(prev, next, priority) {
					return Ok((Position { prev, next }, steps));
				}
				continue;
			}
		}

		let prev_node = prev.and_then(|p| ListNodes::<T>::get(list_id, p));
		let next_node = next.and_then(|n| ListNodes::<T>::get(list_id, n));

		// Detect link inconsistency: are `prev` and `next` actually adjacent?
		let prev_links_match = prev_node
			.as_ref()
			.map_or_else(|| ListHeads::<T>::get(list_id) == next, |pn| pn.next == next);
		let next_links_match = next_node
			.as_ref()
			.map_or_else(|| ListTails::<T>::get(list_id) == prev, |nn| nn.prev == prev);

		if !prev_links_match || !next_links_match {
			// Pick whichever side satisfies its own role in
			// `prev.priority >= priority > next.priority`. If both are priority-correct,
			// trust prev (its cached `next` brings us into the right region).
			// If neither is, trust whichever exists (prev preferred).
			match (prev_node, next_node) {
				(Some(pn), Some(nn)) => {
					if pn.priority >= *priority {
						next = pn.next;
					} else if *priority > nn.priority {
						prev = nn.prev;
					} else {
						next = pn.next;
					}
				},
				(Some(pn), None) => {
					next = pn.next;
				},
				(None, Some(nn)) => {
					prev = nn.prev;
				},
				(None, None) => {
					prev = None;
					next = ListHeads::<T>::get(list_id);
				},
			}

			steps = steps.saturating_add(1);
			if valid(prev, next, priority) {
				return Ok((Position { prev, next }, steps));
			}
			continue;
		}

		// Links are consistent: walk based on priority. Step head-ward if
		// `prev.priority < priority`, tail-ward if `priority <= next.priority`. The
		// `<=` keeps the `>=`/`>` asymmetry.
		let walk_toward = if prev_node.as_ref().is_some_and(|n| n.priority < *priority) {
			Some(Side::Head)
		} else if next_node.as_ref().is_some_and(|n| *priority <= n.priority) {
			Some(Side::Tail)
		} else {
			None
		};

		match walk_toward {
			Some(Side::Head) => {
				next = prev.take();
				prev = prev_node.expect("Side::Head implies prev_node.is_some(); qed").prev;
			},
			Some(Side::Tail) => {
				prev = next.take();
				next = next_node.expect("Side::Tail implies next_node.is_some(); qed").next;
			},
			None => {
				// With consistent links, an invalid position must drive the
				// cursor head- or tail-ward; reaching here means a contract
				// violation. Log it and reset to the head so the loop still
				// terminates.
				defensive!("walk_repair: links consistent but neither side admits priority");
				prev = None;
				next = ListHeads::<T>::get(list_id);
			},
		}

		steps = steps.saturating_add(1);

		if valid(prev, next, priority) {
			return Ok((Position { prev, next }, steps));
		}
	}

	crate::log!(debug, "walk_repair: stale hint exceeded MaxHintRepairSteps ({} steps)", budget,);
	Err(Error::<T>::InvalidPositionHints)
}

/// Insert `item` at `position` in `list_id`. The caller is responsible for
/// ensuring the position is valid; errors if `item` is already in the list.
pub fn insert_at<T: Config>(
	list_id: &T::ListId,
	item: T::ItemId,
	priority: T::Priority,
	position: Position<T::ItemId>,
) -> Result<(), Error<T>> {
	if ListNodes::<T>::contains_key(list_id, item) {
		return Err(Error::<T>::ItemAlreadyExists);
	}
	let new_size = ListSizes::<T>::get(list_id).checked_add(1).ok_or(Error::<T>::ListTooLong)?;

	let prev_node = fetch_neighbor::<T>(list_id, position.prev)?;
	let next_node = fetch_neighbor::<T>(list_id, position.next)?;

	if prev_node.as_ref().is_some_and(|(_, node)| node.next != position.next) ||
		next_node.as_ref().is_some_and(|(_, node)| node.prev != position.prev)
	{
		return Err(Error::<T>::CorruptList);
	}

	// This reads storage and is intentionally debug-only; dispatch weights are
	// benchmarked without `debug_assertions`.
	debug_assert!(is_position_valid::<T>(list_id, &priority, &position));

	link_prev_side::<T>(list_id, prev_node, item);
	link_next_side::<T>(list_id, next_node, item);

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
pub fn remove_at<T: Config>(list_id: &T::ListId, item: T::ItemId) -> Result<(), Error<T>> {
	let Node { prev: removed_prev, next: removed_next, .. } =
		ListNodes::<T>::get(list_id, item).ok_or(Error::<T>::ItemNotFound)?;
	// Defensive: by `try_state` invariant 1, a present node implies `ListSizes >= 1`.
	let new_size = ListSizes::<T>::get(list_id)
		.checked_sub(1)
		.defensive_ok_or(Error::<T>::CorruptList)?;
	if removed_prev == Some(item) || removed_next == Some(item) {
		return Err(Error::<T>::CorruptList);
	}

	let prev_node = fetch_neighbor::<T>(list_id, removed_prev)?;
	let next_node = fetch_neighbor::<T>(list_id, removed_next)?;

	if prev_node.as_ref().is_some_and(|(_, node)| node.next != Some(item)) ||
		next_node.as_ref().is_some_and(|(_, node)| node.prev != Some(item))
	{
		return Err(Error::<T>::CorruptList);
	}

	ListNodes::<T>::remove(list_id, item);

	// Splice the head side: rewrite the left neighbor's `.next` to bypass us
	// (or, if there's no left neighbor, promote `removed_next` to the head —
	// which `set_head` also handles when `removed_next` is `None`, tearing the
	// head pointer down for an emptied list).
	match prev_node {
		Some((p, mut left)) => {
			left.next = removed_next;
			ListNodes::<T>::insert(list_id, p, left);
		},
		None => set_head::<T>(list_id, removed_next),
	}
	// Symmetric on the tail side.
	match next_node {
		Some((n, mut right)) => {
			right.prev = removed_prev;
			ListNodes::<T>::insert(list_id, n, right);
		},
		None => set_tail::<T>(list_id, removed_prev),
	}

	if new_size == 0 {
		ListSizes::<T>::remove(list_id);
	} else {
		ListSizes::<T>::insert(list_id, new_size);
	}
	Ok(())
}
