use super::*;

use pallet_assets::FrozenBalance;

impl<T: Config<I>, I: 'static> FrozenBalance<AssetIdOf<T, I>, AccountIdOf<T>, AssetBalanceOf<T, I>>
	for Pallet<T, I>
{
	fn frozen_balance(
		asset: AssetIdOf<T, I>,
		who: &AccountIdOf<T>,
	) -> Option<AssetBalanceOf<T, I>> {
		FrozenBalances::<T, I>::get(asset, who)
	}

	fn died(asset: AssetIdOf<T, I>, who: &AccountIdOf<T>) {
		FrozenBalances::<T, I>::remove(asset.clone(), who);
		Freezes::<T, I>::remove(asset, who);
	}
}
