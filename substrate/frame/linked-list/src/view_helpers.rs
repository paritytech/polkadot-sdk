//! Read-only helpers used by the [`crate::SortedListInterface`] impl and the
//! `#[pallet::view_functions]` block in `lib.rs`.

use crate::{list, pallet::*};
use alloc::vec::Vec;
use frame::prelude::Get;

/// First `n` items walking from the tail of `list_id`. Returns fewer than `n`
/// if the list has fewer items.
pub(crate) fn iter_from_tail<T: Config>(list_id: &T::ListId, n: u32) -> Vec<T::ItemId> {
	if n == 0 {
		return Vec::new();
	}
	let mut out = Vec::with_capacity(n.min(ListSizes::<T>::get(list_id)) as usize);
	let mut cursor = ListTails::<T>::get(list_id);
	let mut remaining = n;
	while remaining > 0 {
		let Some(item) = cursor else { break };
		let prev = ListNodes::<T>::get(list_id, &item).and_then(|node| node.prev);
		out.push(item);
		cursor = prev;
		remaining = remaining.saturating_sub(1);
	}
	out
}

/// `(prev, next)` insert position for `score` in `list_id`. Walks from the
/// head until `prev.score >= score > next.score` holds. Endpoints come back as
/// `None`.
///
/// O(list size). Off-chain helper; not for hot paths.
pub(crate) fn find_position<T: Config>(
	list_id: &T::ListId,
	score: T::Score,
) -> (Option<T::ItemId>, Option<T::ItemId>) {
	let mut prev: Option<T::ItemId> = None;
	let mut cursor = ListHeads::<T>::get(list_id);
	while let Some(item) = cursor {
		let Some(node) = ListNodes::<T>::get(list_id, &item) else { break };
		if score > node.score {
			return (prev, Some(item));
		}
		prev = Some(item);
		cursor = node.next;
	}
	(prev, None)
}

/// Like [`find_position`], but the result is the position `item` should
/// re-occupy at `new_score` (i.e. `item`'s own node is skipped during the
/// walk). `None` if the item is not in the list.
pub(crate) fn find_re_insert_position<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	new_score: T::Score,
) -> Option<(Option<T::ItemId>, Option<T::ItemId>)> {
	if !ListNodes::<T>::contains_key(list_id, item) {
		return None;
	}
	let mut prev: Option<T::ItemId> = None;
	let mut cursor = ListHeads::<T>::get(list_id);
	while let Some(cur) = cursor {
		if &cur == item {
			cursor = ListNodes::<T>::get(list_id, &cur).and_then(|n| n.next);
			continue;
		}
		let Some(node) = ListNodes::<T>::get(list_id, &cur) else { break };
		if new_score > node.score {
			return Some((prev, Some(cur)));
		}
		prev = Some(cur);
		cursor = node.next;
	}
	Some((prev, None))
}

/// Steps the on-chain repair walk would take from `(hint_prev, hint_next)` to
/// reach the position for `score`. `0` means the hint is already valid; any
/// value greater than `T::MaxHintRepairSteps` means a dispatch with the same
/// hint would fail.
pub(crate) fn repair_steps_needed<T: Config>(
	list_id: &T::ListId,
	score: T::Score,
	hint_prev: Option<T::ItemId>,
	hint_next: Option<T::ItemId>,
) -> u32 {
	match list::walk_repair::<T>(list_id, &score, hint_prev, hint_next) {
		Ok((_, _, steps)) => steps,
		Err(_) => T::MaxHintRepairSteps::get().saturating_add(1),
	}
}
