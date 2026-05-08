//! Implementation of the [`Pallet::relist`] dispatchable.

use crate::{pallet::*, SortedListInterface};
use frame::prelude::*;

/// Refresh `(list_id, item)`'s stored score from [`crate::ScoreProvider`] and
/// reposition it via [`SortedListInterface::re_insert`]. Returns the number of
/// hint-repair steps actually walked.
pub(crate) fn relist_internal<T: Config>(
	list_id: &T::ListId,
	item: &T::ItemId,
	hint_prev: Option<T::ItemId>,
	hint_next: Option<T::ItemId>,
) -> Result<u32, Error<T>> {
	let stored = ListNodes::<T>::get(list_id, item).ok_or(Error::<T>::ItemNotFound)?;
	let Some(real_score) =
		<T::ScoreProvider as crate::ScoreProvider<T::ListId, T::ItemId>>::score(list_id, item)
	else {
		<Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::remove(list_id, item)?;
		return Ok(0);
	};

	if stored.score == real_score {
		return Ok(0);
	}
	crate::log!(
		debug,
		"relist: score drift detected, repositioning (old={:?}, new={:?})",
		stored.score,
		real_score,
	);

	let steps = <Pallet<T> as SortedListInterface<T::ListId, T::ItemId>>::re_insert(
		list_id.clone(),
		item.clone(),
		real_score,
		hint_prev,
		hint_next,
	)?;

	Pallet::<T>::deposit_event(Event::Relisted {
		list_id: list_id.clone(),
		item: item.clone(),
		new_score: real_score,
	});
	Ok(steps)
}
