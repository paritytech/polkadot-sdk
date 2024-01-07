use crate::{
	traits::sufficients::{Inspect, Mutate},
	Asset, Config, Error, Pallet,
};

impl<T: Config<I>, I: 'static> Inspect<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn is_sufficient(asset_id: <T as Config<I>>::AssetId) -> bool {
		Asset::<T, I>::get(asset_id).map(|asset| asset.is_sufficient).unwrap_or(false)
	}
}

impl<T: Config<I>, I: 'static> Mutate<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn set_sufficient(
		asset_id: <T as Config<I>>::AssetId,
		is_sufficient: bool,
	) -> sp_runtime::DispatchResult {
		Asset::<T, I>::try_mutate(asset_id, |maybe_asset| {
			if let Some(asset) = maybe_asset {
				asset.is_sufficient = is_sufficient;
				Ok(())
			} else {
				Err(Error::<T, I>::Unknown)?
			}
		})
	}
}
