//! Storage primitives for the sorted doubly-linked list.
//!
//! [`Node`] is the per-item storage value. [`insert_at`], [`remove_at`] and
//! [`walk_repair`] mutate or read the per-list [`ListNodes`], [`ListHeads`],
//! [`ListTails`] and [`ListSizes`] storage maps and are wrapped by the trait
//! impl in [`super::sorted_list_interface`].

#[allow(clippy::wildcard_imports)]
use crate::pallet::*;
use frame::{deps::frame_support::traits::DefensiveOption, prelude::*};

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

/// Whether `(prev, next)` is a valid insert position for `priority` in `list_id`.
///
/// Checks that the link structure is consistent with the list's head/tail
/// pointers and that `prev.priority >= priority > next.priority` (with endpoints
/// treated as `+inf` / `-inf`). The `>=`/`>` asymmetry places same-priority
/// inserts on the tail side of their cluster, yielding LIFO under tail-first
/// iteration.
pub(crate) fn is_position_valid<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	prev: &Option<T::ItemId>,
	next: &Option<T::ItemId>,
) -> bool {
	let prev_ok = match prev {
		None => &ListHeads::<T>::get(list_id) == next,
		Some(p) => match ListNodes::<T>::get(list_id, p) {
			Some(n) => &n.next == next && n.priority >= *priority,
			None => return false,
		},
	};
	if !prev_ok {
		return false;
	}

	match next {
		None => &ListTails::<T>::get(list_id) == prev,
		Some(n) => match ListNodes::<T>::get(list_id, n) {
			Some(node) => &node.prev == prev && *priority > node.priority,
			None => false,
		},
	}
}

/// Priority-only half of [`is_position_valid`]. Skips the link-consistency check
/// and is used by `re_insert`'s in-place fast path, where the existing links
/// are valid by construction.
pub(crate) fn neighbor_priorities_admit<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	prev: &Option<T::ItemId>,
	next: &Option<T::ItemId>,
) -> bool {
	let prev_ok = match prev {
		None => true,
		Some(p) => ListNodes::<T>::get(list_id, p).is_some_and(|n| n.priority >= *priority),
	};
	let next_ok = match next {
		None => true,
		Some(n) => ListNodes::<T>::get(list_id, n).is_some_and(|node| *priority > node.priority),
	};
	prev_ok && next_ok
}

/// Walk from `(hint_prev, hint_next)` toward the correct insert position for
/// `priority`, taking at most `MaxHintRepairSteps` steps.
///
/// Returns the corrected `(prev, next, steps_taken)`, or `InvalidPositionHints`
/// if the budget is exhausted before a valid position is reached.
pub(crate) fn walk_repair<T: Config>(
	list_id: &T::ListId,
	priority: &T::Priority,
	mut prev: Option<T::ItemId>,
	mut next: Option<T::ItemId>,
) -> Result<(Option<T::ItemId>, Option<T::ItemId>, u32), Error<T>> {
	if is_position_valid::<T>(list_id, priority, &prev, &next) {
		return Ok((prev, next, 0));
	}

	let budget = T::MaxHintRepairSteps::get();
	let mut steps = 0u32;

	while steps < budget {
		// Clamp dangling hints (caller referenced a removed node) to `None`.
		if let Some(p) = prev.as_ref() {
			if !ListNodes::<T>::contains_key(list_id, p) {
				prev = None;
				steps = steps.saturating_add(1);
				if is_position_valid::<T>(list_id, priority, &prev, &next) {
					return Ok((prev, next, steps));
				}
				continue;
			}
		}
		if let Some(n) = next.as_ref() {
			if !ListNodes::<T>::contains_key(list_id, n) {
				next = None;
				steps = steps.saturating_add(1);
				if is_position_valid::<T>(list_id, priority, &prev, &next) {
					return Ok((prev, next, steps));
				}
				continue;
			}
		}

		let prev_node = prev.as_ref().and_then(|p| ListNodes::<T>::get(list_id, p));
		let next_node = next.as_ref().and_then(|n| ListNodes::<T>::get(list_id, n));

		// Detect link inconsistency: are `prev` and `next` actually adjacent?
		let prev_links_match = match &prev_node {
			Some(pn) => pn.next == next,
			None => ListHeads::<T>::get(list_id) == next,
		};
		let next_links_match = match &next_node {
			Some(nn) => nn.prev == prev,
			None => ListTails::<T>::get(list_id) == prev,
		};

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
			if is_position_valid::<T>(list_id, priority, &prev, &next) {
				return Ok((prev, next, steps));
			}
			continue;
		}

		// Links are consistent: walk based on priority. Walk head-ward if
		// `prev.priority < priority`, tail-ward if `priority <= next.priority`. The
		// `<=` keeps the `>=`/`>` asymmetry.
		let go_head = prev_node.as_ref().is_some_and(|n| n.priority < *priority);
		let go_tail = next_node.as_ref().is_some_and(|n| *priority <= n.priority);

		if go_head {
			next = prev.take();
			prev = prev_node.expect("go_head implies prev_node.is_some(); qed").prev;
		} else if go_tail {
			prev = next.take();
			next = next_node.expect("go_tail implies next_node.is_some(); qed").next;
		} else {
			// With consistent links, an invalid position must trigger `go_head`
			// or `go_tail`; reaching here means a contract violation. Log it
			// and reset to the head so the loop still terminates.
			defensive!("walk_repair: links consistent but neither side admits priority");
			prev = None;
			next = ListHeads::<T>::get(list_id);
		}

		steps = steps.saturating_add(1);

		if is_position_valid::<T>(list_id, priority, &prev, &next) {
			return Ok((prev, next, steps));
		}
	}

	crate::log!(debug, "walk_repair: stale hint exceeded MaxHintRepairSteps ({} steps)", budget,);
	Err(Error::<T>::InvalidPositionHints)
}

/// Insert `item` between `prev` and `next` in `list_id`. The caller is
/// responsible for ensuring the position is valid; errors if `item` is already
/// in the list.
pub(crate) fn insert_at<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	priority: T::Priority,
	prev: Option<T::ItemId>,
	next: Option<T::ItemId>,
) -> Result<(), Error<T>> {
	if ListNodes::<T>::contains_key(list_id, item) {
		return Err(Error::<T>::ItemAlreadyExists);
	}
	let new_size = ListSizes::<T>::get(list_id).checked_add(1).ok_or(Error::<T>::ListTooLong)?;
	debug_assert!(is_position_valid::<T>(list_id, &priority, &prev, &next));

	match prev.as_ref() {
		Some(p) => ListNodes::<T>::mutate(list_id, p, |maybe| {
			if let Some(n) = maybe {
				n.next = Some(item.clone());
			}
		}),
		None => ListHeads::<T>::insert(list_id, item.clone()),
	}

	match next.as_ref() {
		Some(n) => ListNodes::<T>::mutate(list_id, n, |maybe| {
			if let Some(node) = maybe {
				node.prev = Some(item.clone());
			}
		}),
		None => ListTails::<T>::insert(list_id, item.clone()),
	}

	ListNodes::<T>::insert(list_id, item, Node { prev, next, priority });
	ListSizes::<T>::insert(list_id, new_size);
	Ok(())
}

/// Remove `item` from `list_id`. Cleans up the list's
/// `ListHeads`/`ListTails`/`ListSizes` rows when it becomes empty. Errors if
/// `item` is not in the list.
pub(crate) fn remove_at<T: Config>(list_id: &T::ListId, item: &T::ItemId) -> Result<(), Error<T>> {
	let Node { prev: removed_prev, next: removed_next, .. } =
		ListNodes::<T>::get(list_id, item).ok_or(Error::<T>::ItemNotFound)?;
	// Defensive: by `try_state` invariant 1, a present node implies `ListSizes >= 1`.
	let new_size = ListSizes::<T>::get(list_id)
		.checked_sub(1)
		.defensive_ok_or(Error::<T>::CorruptList)?;
	ListNodes::<T>::remove(list_id, item);

	match (removed_prev, removed_next) {
		(Some(p), Some(n)) => {
			ListNodes::<T>::mutate(list_id, &p, |maybe| {
				if let Some(left) = maybe {
					left.next = Some(n.clone());
				}
			});
			ListNodes::<T>::mutate(list_id, &n, |maybe| {
				if let Some(right) = maybe {
					right.prev = Some(p);
				}
			});
		},
		(Some(p), None) => {
			ListNodes::<T>::mutate(list_id, &p, |maybe| {
				if let Some(left) = maybe {
					left.next = None;
				}
			});
			ListTails::<T>::insert(list_id, p);
		},
		(None, Some(n)) => {
			ListNodes::<T>::mutate(list_id, &n, |maybe| {
				if let Some(right) = maybe {
					right.prev = None;
				}
			});
			ListHeads::<T>::insert(list_id, n);
		},
		(None, None) => {
			ListHeads::<T>::remove(list_id);
			ListTails::<T>::remove(list_id);
		},
	}

	if new_size == 0 {
		ListSizes::<T>::remove(list_id);
	} else {
		ListSizes::<T>::insert(list_id, new_size);
	}
	Ok(())
}
