// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::impls::AccountIdOf;
use core::marker::PhantomData;
use cumulus_primitives_core::{IsSystem, ParaId};
use frame_support::{
	traits::{fungibles::Inspect, tokens::ConversionToAssetBalance, Contains, ContainsPair},
	weights::Weight,
};
use sp_runtime::traits::Get;
use xcm::latest::prelude::*;

/// A `ChargeFeeInFungibles` implementation that converts the output of
/// a given WeightToFee implementation an amount charged in
/// a particular assetId from pallet-assets
pub struct AssetFeeAsExistentialDepositMultiplier<
	Runtime,
	WeightToFee,
	BalanceConverter,
	AssetInstance: 'static,
>(PhantomData<(Runtime, WeightToFee, BalanceConverter, AssetInstance)>);
impl<CurrencyBalance, Runtime, WeightToFee, BalanceConverter, AssetInstance>
	cumulus_primitives_utility::ChargeWeightInFungibles<
		AccountIdOf<Runtime>,
		pallet_assets::Pallet<Runtime, AssetInstance>,
	> for AssetFeeAsExistentialDepositMultiplier<Runtime, WeightToFee, BalanceConverter, AssetInstance>
where
	Runtime: pallet_assets::Config<AssetInstance>,
	WeightToFee: frame_support::weights::WeightToFee<Balance = CurrencyBalance>,
	BalanceConverter: ConversionToAssetBalance<
		CurrencyBalance,
		<Runtime as pallet_assets::Config<AssetInstance>>::AssetId,
		<Runtime as pallet_assets::Config<AssetInstance>>::Balance,
	>,
{
	fn charge_weight_in_fungibles(
		asset_id: <pallet_assets::Pallet<Runtime, AssetInstance> as Inspect<
			AccountIdOf<Runtime>,
		>>::AssetId,
		weight: Weight,
	) -> Result<
		<pallet_assets::Pallet<Runtime, AssetInstance> as Inspect<AccountIdOf<Runtime>>>::Balance,
		XcmError,
	> {
		let amount = WeightToFee::weight_to_fee(&weight);
		// If the amount gotten is not at least the ED, then make it be the ED of the asset
		// This is to avoid burning assets and decreasing the supply
		let asset_amount = BalanceConverter::to_asset_balance(amount, asset_id)
			.map_err(|_| XcmError::TooExpensive)?;
		Ok(asset_amount)
	}
}

/// Accepts an asset if it is a native asset from a particular `Location`.
pub struct ConcreteNativeAssetFrom<LocationValue>(PhantomData<LocationValue>);
impl<LocationValue: Get<Location>> ContainsPair<Asset, Location>
	for ConcreteNativeAssetFrom<LocationValue>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::filter_asset_location",
			"ConcreteNativeAsset asset: {:?}, origin: {:?}, location: {:?}",
			asset, origin, LocationValue::get());
		asset.id.0 == *origin && origin == &LocationValue::get()
	}
}

pub struct RelayOrOtherSystemParachains<
	SystemParachainMatcher: Contains<Location>,
	Runtime: parachain_info::Config,
> {
	_runtime: PhantomData<(SystemParachainMatcher, Runtime)>,
}
impl<SystemParachainMatcher: Contains<Location>, Runtime: parachain_info::Config> Contains<Location>
	for RelayOrOtherSystemParachains<SystemParachainMatcher, Runtime>
{
	fn contains(l: &Location) -> bool {
		let self_para_id: u32 = parachain_info::Pallet::<Runtime>::get().into();
		if let (0, [Parachain(para_id)]) = l.unpack() {
			if *para_id == self_para_id {
				return false
			}
		}
		matches!(l.unpack(), (1, [])) || SystemParachainMatcher::contains(l)
	}
}

/// Contains all sibling system parachains, including the one where this matcher is used.
///
/// This structure can only be used at a parachain level. In the Relay Chain, please use
/// the `xcm_builder::IsChildSystemParachain` matcher.
pub struct AllSiblingSystemParachains;
impl Contains<Location> for AllSiblingSystemParachains {
	fn contains(l: &Location) -> bool {
		log::trace!(target: "xcm::contains", "AllSiblingSystemParachains location: {:?}", l);
		match l.unpack() {
			// System parachain
			(1, [Parachain(id)]) => ParaId::from(*id).is_system(),
			// Everything else
			_ => false,
		}
	}
}

/// Accepts an asset if it is a concrete asset from the system (Relay Chain or system parachain).
pub struct ConcreteAssetFromSystem<AssetLocation>(PhantomData<AssetLocation>);
impl<AssetLocation: Get<Location>> ContainsPair<Asset, Location>
	for ConcreteAssetFromSystem<AssetLocation>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "ConcreteAssetFromSystem asset: {:?}, origin: {:?}", asset, origin);
		let is_system = match origin.unpack() {
			// The Relay Chain
			(1, []) => true,
			// System parachain
			(1, [Parachain(id)]) => ParaId::from(*id).is_system(),
			// Others
			_ => false,
		};
		asset.id.0 == AssetLocation::get() && is_system
	}
}

/// Filter to check if a given location is the parent Relay Chain or a sibling parachain.
///
/// This type should only be used within the context of a parachain, since it does not verify that
/// the parent is indeed a Relay Chain.
pub struct ParentRelayOrSiblingParachains;
impl Contains<Location> for ParentRelayOrSiblingParachains {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []) | (1, [Parachain(_)]))
	}
}

/// Filter to check if a given `target` location represents the same AccountId32 as `origin`,
/// but coming from another sibling system chain.
///
/// This type should only be used within the context of a parachain, to allow accounts on system
/// chains to Alias to the same accounts on the local chain.
pub struct AliasAccountId32FromSiblingSystemChain;
impl ContainsPair<Location, Location> for AliasAccountId32FromSiblingSystemChain {
	fn contains(origin: &Location, target: &Location) -> bool {
		let result = match origin.unpack() {
			// `origin` is AccountId32 on sibling system parachain
			(1, [Parachain(para_id), AccountId32 { network: _, id: origin }])
				if ParaId::from(*para_id).is_system() =>
			{
				match target.unpack() {
					// `target` is local AccountId32 and matches `origin` remote account
					(0, [AccountId32 { network: _, id: target }]) => target.eq(origin),
					_ => false,
				}
			},
			_ => false,
		};
		log::trace!(
			target: "xcm::contains",
			"AliasAccountId32FromSiblingSystemChain origin: {:?}, target: {:?}, result {:?}",
			origin, target, result,
		);
		result
	}
}

#[cfg(test)]
mod tests {
	use frame_support::{parameter_types, traits::Contains};

	use super::{
		AliasAccountId32FromSiblingSystemChain, AllSiblingSystemParachains, Asset,
		ConcreteAssetFromSystem, ContainsPair, GeneralIndex, Here, Location, PalletInstance,
		Parachain, Parent,
	};
	use polkadot_primitives::LOWEST_PUBLIC_ID;
	use xcm::latest::prelude::*;

	parameter_types! {
		pub const RelayLocation: Location = Location::parent();
	}

	#[test]
	fn concrete_asset_from_relay_works() {
		let expected_asset: Asset = (Parent, 1000000).into();
		let expected_origin: Location = (Parent, Here).into();

		assert!(<ConcreteAssetFromSystem<RelayLocation>>::contains(
			&expected_asset,
			&expected_origin
		));
	}

	#[test]
	fn concrete_asset_from_sibling_system_para_fails_for_wrong_asset() {
		let unexpected_assets: Vec<Asset> = vec![
			(Here, 1000000).into(),
			((PalletInstance(50), GeneralIndex(1)), 1000000).into(),
			((Parent, Parachain(1000), PalletInstance(50), GeneralIndex(1)), 1000000).into(),
		];
		let expected_origin: Location = (Parent, Parachain(1000)).into();

		unexpected_assets.iter().for_each(|asset| {
			assert!(!<ConcreteAssetFromSystem<RelayLocation>>::contains(asset, &expected_origin));
		});
	}

	#[test]
	fn concrete_asset_from_sibling_system_para_works_for_correct_asset() {
		// (para_id, expected_result)
		let test_data = vec![
			(0, true),
			(1, true),
			(1000, true),
			(1999, true),
			(2000, false), // Not a System Parachain
			(2001, false), // Not a System Parachain
		];

		let expected_asset: Asset = (Parent, 1000000).into();

		for (para_id, expected_result) in test_data {
			let origin: Location = (Parent, Parachain(para_id)).into();
			assert_eq!(
				expected_result,
				<ConcreteAssetFromSystem<RelayLocation>>::contains(&expected_asset, &origin)
			);
		}
	}

	#[test]
	fn all_sibling_system_parachains_works() {
		// system parachain
		assert!(AllSiblingSystemParachains::contains(&Location::new(1, [Parachain(1)])));
		// non-system parachain
		assert!(!AllSiblingSystemParachains::contains(&Location::new(
			1,
			[Parachain(LOWEST_PUBLIC_ID.into())]
		)));
		// when used at relay chain
		assert!(!AllSiblingSystemParachains::contains(&Location::new(0, [Parachain(1)])));
		// when used with non-parachain
		assert!(!AllSiblingSystemParachains::contains(&Location::new(1, [OnlyChild])));
	}

	#[test]
	fn alias_accountid32_from_sibling_system_parachains() {
		let acc_42 = AccountId32 { network: None, id: [42u8; 32] };
		let acc_13 = AccountId32 { network: None, id: [13u8; 32] };
		// origin acc_42 on sibling system parachain aliases into local acc_42
		assert!(AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(0, [acc_42])
		));
		// if target is not local account, always fails
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(0, [])
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(0, [Parachain(1)])
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(0, [GeneralIndex(42)])
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(1, [acc_42])
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_42]),
			&Location::new(2, [acc_42])
		));
		// origin acc_13 on sibling system parachain CANNOT alias into local acc_42
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(1), acc_13]),
			&Location::new(0, [acc_42])
		));
		// origin acc_42 on sibling non-system parachain CANNOT alias into local acc_42
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(1, [Parachain(LOWEST_PUBLIC_ID.into()), acc_42]),
			&Location::new(0, [acc_42])
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(0, [acc_13]),
			&Location::new(0, [acc_13]),
		));
		assert!(!AliasAccountId32FromSiblingSystemChain::contains(
			&Location::new(0, [acc_42]),
			&Location::new(1, [Parachain(1), acc_42]),
		));
	}
}
