use crate::{
	traits::sufficients::{Inspect, Mutate},
	Asset, Config, Pallet,
};

impl<T: Config<I>, I: 'static> Inspect<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn is_sufficient(asset_id: <T as Config<I>>::AssetId) -> bool {
		Asset::<T, I>::get(asset_id).map(|asset| asset.is_sufficient).unwrap_or(false)
	}
}

impl<T: Config<I>, I: 'static> Mutate<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn make_sufficient(asset_id: <T as Config<I>>::AssetId) -> sp_runtime::DispatchResult {
		Pallet::<T, I>::do_set_sufficiency(asset_id, true)
	}

	fn make_insufficient(asset_id: <T as Config<I>>::AssetId) -> sp_runtime::DispatchResult {
		Pallet::<T, I>::do_set_sufficiency(asset_id, false)
	}
}
