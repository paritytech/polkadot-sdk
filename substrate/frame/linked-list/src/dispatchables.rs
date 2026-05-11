//! Implementation of the [`Pallet::reprioritize`] dispatchable.

use crate::{pallet::*, SortedListInterface};
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
	let Some(real_priority) = <T::PriorityProvider as crate::PriorityProvider<
		T::ListId,
		T::ItemId,
	>>::priority(list_id, item) else {
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
