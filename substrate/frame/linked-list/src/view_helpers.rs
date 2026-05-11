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

//! Read-only helpers used by the [`crate::SortedListInterface`] impl and the
//! `#[pallet::view_functions]` block in `lib.rs`.

use crate::{list, pallet::*, Position};
use alloc::vec::Vec;
use frame::prelude::Get;

/// First `n` items walking from the tail of `list_id`. Returns fewer than `n`
/// if the list has fewer items.
pub fn iter_from_tail<T: Config>(list_id: &T::ListId, n: u32) -> Vec<T::ItemId> {
	if n == 0 {
		return Vec::new();
	}
	let mut out = Vec::with_capacity(n.min(ListSizes::<T>::get(list_id)) as usize);
	let mut cursor = ListTails::<T>::get(list_id);
	for _ in 0..n {
		let Some(item) = cursor else { break };
		let prev = ListNodes::<T>::get(list_id, &item).and_then(|node| node.prev);
		out.push(item);
		cursor = prev;
	}
	out
}

/// Insert position for `priority` in `list_id`. Walks from the head until
/// `prev.priority >= priority > next.priority` holds. Endpoints encoded as
/// `None`.
///
/// O(list size). Off-chain helper; not for hot paths.
pub fn find_position<T: Config>(list_id: &T::ListId, priority: T::Priority) -> Position<T::ItemId> {
	let mut prev: Option<T::ItemId> = None;
	let mut cursor = ListHeads::<T>::get(list_id);
	while let Some(item) = cursor {
		let Some(node) = ListNodes::<T>::get(list_id, &item) else { break };
		if priority > node.priority {
			return Position { prev, next: Some(item) };
		}
		prev = Some(item);
		cursor = node.next;
	}
	Position { prev, next: None }
}

/// Like [`find_position`], but the result is the position `item` should
/// re-occupy at `new_priority` (i.e. `item`'s own node is skipped during the
/// walk). `None` if the item is not in the list.
pub fn find_re_insert_position<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	new_priority: T::Priority,
) -> Option<Position<T::ItemId>> {
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
		if new_priority > node.priority {
			return Some(Position { prev, next: Some(cur) });
		}
		prev = Some(cur);
		cursor = node.next;
	}
	Some(Position { prev, next: None })
}

/// Steps the on-chain repair walk would take from `hint` to reach the position
/// for `priority`. `0` means the hint is already valid; any value greater than
/// `T::MaxHintRepairSteps` means a dispatch with the same hint would fail.
pub fn repair_steps_needed<T: Config>(
	list_id: &T::ListId,
	priority: T::Priority,
	hint: Position<T::ItemId>,
) -> u32 {
	match list::walk_repair::<T>(list_id, &priority, hint) {
		Ok((_, steps)) => steps,
		Err(_) => T::MaxHintRepairSteps::get().saturating_add(1),
	}
}
