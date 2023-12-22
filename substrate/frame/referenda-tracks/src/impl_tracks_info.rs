use super::*;

impl<T: Config<I>, I> pallet_referenda::TracksInfo<BalanceOf<T, I>, BlockNumberFor<T>>
	for Pallet<T, I>
{
	type Id = T::TrackId;
	type RuntimeOrigin = <T::RuntimeOrigin as OriginTrait>::PalletsOrigin;
	type TracksIter = TracksIter<T, I>;

	fn tracks() -> Self::TracksIter {
		Tracks::<T, I>::iter().map(|(id, info)| Cow::Owned(Track { id, info }))
	}
	fn track_for(origin: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		OriginToTrackId::<T, I>::get(origin).ok_or(())
	}
	fn tracks_ids() -> Vec<Self::Id> {
		TracksIds::<T, I>::get().into_inner()
	}
	fn info(id: Self::Id) -> Option<Cow<'static, TrackInfoOf<T, I>>> {
		Tracks::<T, I>::get(id).map(Cow::Owned)
	}
}

impl<T: Config<I>, I: 'static> Get<Vec<TrackOf<T, I>>> for crate::Pallet<T, I> {
	fn get() -> Vec<TrackOf<T, I>> {
		// expensive but it doesn't seem to be used anywhere
		<Pallet<T, I> as pallet_referenda::TracksInfo<BalanceOf<T, I>, BlockNumberFor<T>>>::tracks()
			.map(|t| t.into_owned())
			.collect()
	}
}
