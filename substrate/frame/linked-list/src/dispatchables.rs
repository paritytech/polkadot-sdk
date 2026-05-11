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

//! Implementation of the [`Pallet::reprioritize`] dispatchable.

use crate::{pallet::*, PriorityProvider, SortedListInterface};
use frame::prelude::*;

/// Refresh `(list_id, item)`'s stored priority from [`crate::PriorityProvider`] and
/// reposition it via [`SortedListInterface::re_insert`]. Returns the number of
/// hint-repair steps actually walked.
pub(crate) fn reprioritize_internal<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	hint_prev: Option<T::ItemId>,
	hint_next: Option<T::ItemId>,
) -> Result<u32, Error<T>> {
	let stored = ListNodes::<T>::get(list_id, item).ok_or(Error::<T>::ItemNotFound)?;
	let Some(real_priority) = T::PriorityProvider::priority(list_id, item) else {
		<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::remove(list_id, item)?;
		return Ok(0);
	};

	if stored.priority == real_priority {
		return Ok(0);
	}
	crate::log!(
		debug,
		"reprioritize: priority drift detected, repositioning (old={:?}, new={:?})",
		stored.priority,
		real_priority,
	);

	let steps = <Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::re_insert(
		list_id.clone(),
		item.clone(),
		real_priority,
		hint_prev,
		hint_next,
	)?;

	Pallet::<T>::deposit_event(Event::Reprioritized {
		list_id: list_id.clone(),
		item: item.clone(),
		new_priority: real_priority,
	});
	Ok(steps)
}
