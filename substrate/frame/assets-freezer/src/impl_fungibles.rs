use super::*;

use frame_support::traits::fungibles::{Inspect, InspectFreeze};
use sp_core::Get;

// Implement fungibles::Inspect as it is required. To do so, we'll re-export
// all of `pallet-assets`' implementation of the same trait.
impl<T: Config<I>, I: 'static> Inspect<AccountIdOf<T>> for Pallet<T, I> {
	type AssetId = AssetIdOf<T, I>;
	type Balance = AssetBalanceOf<T, I>;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_issuance(asset)
	}

	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::minimum_balance(asset)
	}

	fn total_balance(asset: Self::AssetId, who: &AccountIdOf<T>) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::total_balance(asset, who)
	}

	fn balance(asset: Self::AssetId, who: &AccountIdOf<T>) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::balance(asset, who)
	}

	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		preservation: frame_support::traits::tokens::Preservation,
		force: frame_support::traits::tokens::Fortitude,
	) -> Self::Balance {
		pallet_assets::Pallet::<T, I>::reducible_balance(asset, who, preservation, force)
	}

	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
		provenance: frame_support::traits::tokens::Provenance,
	) -> frame_support::traits::tokens::DepositConsequence {
		pallet_assets::Pallet::<T, I>::can_deposit(asset, who, amount, provenance)
	}

	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountIdOf<T>,
		amount: Self::Balance,
	) -> frame_support::traits::tokens::WithdrawConsequence<Self::Balance> {
		pallet_assets::Pallet::<T, I>::can_withdraw(asset, who, amount)
	}

	fn asset_exists(asset: Self::AssetId) -> bool {
		pallet_assets::Pallet::<T, I>::asset_exists(asset)
	}
}

impl<T: Config<I>, I: 'static> InspectFreeze<AccountIdOf<T>> for Pallet<T, I> {
	type Id = T::RuntimeFreezeReason;

	fn balance_frozen(asset: Self::AssetId, id: &Self::Id, who: &AccountIdOf<T>) -> Self::Balance {
		let (_, balance) = Freezes::<T, I>::get(asset, who)
			.into_iter()
			.find(|(freeze_id, _)| freeze_id == id)
			.unwrap_or((id.clone(), Default::default()));

		balance.clone()
	}

	fn can_freeze(asset: Self::AssetId, id: &Self::Id, who: &AccountIdOf<T>) -> bool {
		let freezes = Freezes::<T, I>::get(asset, who);
		freezes.len()
			< T::MaxFreezes::get()
				.try_into()
				.expect("MaxFreezes is the same type as S within Freezes<S>; qed")
			|| freezes.iter().any(|(freeze_id, _)| freeze_id == id)
	}
}
