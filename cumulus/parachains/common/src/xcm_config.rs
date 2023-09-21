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
use codec::Decode;
use core::marker::PhantomData;
use frame_support::{
	ensure,
	traits::{
		fungibles::Inspect, tokens::ConversionToAssetBalance, Contains, ContainsPair,
		ProcessMessageError,
	},
	weights::Weight,
	DefaultNoBound,
};
use log;
use sp_runtime::traits::Get;
use xcm::latest::prelude::*;
use xcm_builder::{CreateMatcher, ExporterFor, MatchXcm};
use xcm_executor::traits::ShouldExecute;

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
	AccountIdOf<Runtime>:
		From<polkadot_primitives::AccountId> + Into<polkadot_primitives::AccountId>,
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

/// Accepts an asset if it is a native asset from a particular `MultiLocation`.
pub struct ConcreteNativeAssetFrom<Location>(PhantomData<Location>);
impl<Location: Get<MultiLocation>> ContainsPair<MultiAsset, MultiLocation>
	for ConcreteNativeAssetFrom<Location>
{
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		log::trace!(target: "xcm::filter_asset_location",
			"ConcreteNativeAsset asset: {:?}, origin: {:?}, location: {:?}",
			asset, origin, Location::get());
		matches!(asset.id, Concrete(ref id) if id == origin && origin == &Location::get())
	}
}

/// Trait for matching `Location`.
pub trait MatchesLocation<Location> {
	fn matches(&self, location: &Location) -> bool;
}

/// Simple `MultiLocation` filter utility.
#[derive(Debug, DefaultNoBound)]
pub struct LocationFilter<Location> {
	/// Requested location equals to `Location`.
	pub equals_any: sp_std::vec::Vec<Location>,
	/// Requested location starts with `Location`.
	pub starts_with_any: sp_std::vec::Vec<Location>,
}

impl<Location> LocationFilter<Location> {
	pub fn add_equals(mut self, filter: Location) -> Self {
		self.equals_any.push(filter);
		self
	}
	pub fn add_starts_with(mut self, filter: Location) -> Self {
		self.starts_with_any.push(filter);
		self
	}
}

/// `MatchesLocation` implementation which works with `MultiLocation`.
impl MatchesLocation<MultiLocation> for LocationFilter<MultiLocation> {
	fn matches(&self, location: &MultiLocation) -> bool {
		for filter in &self.equals_any {
			if location.eq(filter) {
				return true
			}
		}
		for filter in &self.starts_with_any {
			if location.starts_with(filter) {
				return true
			}
		}
		false
	}
}

/// `MatchesLocation` implementation which works with `InteriorMultiLocation`.
impl MatchesLocation<InteriorMultiLocation> for LocationFilter<InteriorMultiLocation> {
	fn matches(&self, location: &InteriorMultiLocation) -> bool {
		for filter in &self.equals_any {
			if location.eq(filter) {
				return true
			}
		}
		for filter in &self.starts_with_any {
			if location.starts_with(filter) {
				return true
			}
		}
		false
	}
}

/// `FilteredNetworkExportTable` is adapter for `ExporterFor` implementation
/// which tries to find (`bridge_location`, `fees`) for requested `network` and `remote_location`.
///
/// Inspired by `xcm_builder::NetworkExportTable`:
/// the main difference is that `NetworkExportTable` does not check `remote_location`.
pub struct FilteredNetworkExportTable<T>(sp_std::marker::PhantomData<T>);
impl<
		T: Get<
			sp_std::vec::Vec<(
				NetworkId,
				LocationFilter<InteriorMultiLocation>,
				MultiLocation,
				Option<MultiAsset>,
			)>,
		>,
	> ExporterFor for FilteredNetworkExportTable<T>
{
	fn exporter_for(
		network: &NetworkId,
		remote_location: &InteriorMultiLocation,
		_: &Xcm<()>,
	) -> Option<(MultiLocation, Option<MultiAsset>)> {
		T::get()
			.into_iter()
			.find(|(ref j, location_filter, ..)| {
				j == network && location_filter.matches(remote_location)
			})
			.map(|(_, _, bridge_location, p)| (bridge_location, p))
	}
}

/// Allows execution from `origin` (only with `OriginKind::Xcm`) if it is contained in
/// `AllowedOrigin` and if it is just a straight `Transact` which passes `AllowedCall` matcher.
pub struct AllowUnpaidTransactsFrom<RuntimeCall, AllowedCall, AllowedOrigin>(
	sp_std::marker::PhantomData<(RuntimeCall, AllowedCall, AllowedOrigin)>,
);
impl<
		RuntimeCall: Decode,
		AllowedCall: Contains<RuntimeCall>,
		AllowedOrigin: Contains<MultiLocation>,
	> ShouldExecute for AllowUnpaidTransactsFrom<RuntimeCall, AllowedCall, AllowedOrigin>
{
	fn should_execute<Call>(
		origin: &MultiLocation,
		instructions: &mut [Instruction<Call>],
		_max_weight: Weight,
		_properties: &mut xcm_executor::traits::Properties,
	) -> Result<(), ProcessMessageError> {
		log::trace!(
			target: "xcm::barriers",
			"AllowUnpaidTransactFrom origin: {:?}, instructions: {:?}, max_weight: {:?}, properties: {:?}",
			origin, instructions, _max_weight, _properties,
		);

		// we only allow from configured origins
		ensure!(AllowedOrigin::contains(origin), ProcessMessageError::Unsupported);

		// we expect an XCM program with single `Transact` call
		instructions
			.matcher()
			.assert_remaining_insts(1)?
			.match_next_inst(|inst| match inst {
				Transact { origin_kind: OriginKind::Xcm, call: encoded_call, .. } => {
					// Generic `Call` to `RuntimeCall` conversion - don't know if there's a way to
					// do that properly?
					let runtime_call: RuntimeCall = encoded_call
						.clone()
						.into::<RuntimeCall>()
						.try_into()
						.map_err(|_| ProcessMessageError::BadFormat)?;

					ensure!(AllowedCall::contains(&runtime_call), ProcessMessageError::Unsupported);

					Ok(())
				},
				_ => Err(ProcessMessageError::BadFormat),
			})?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	#[test]
	fn filtered_network_export_table_works() {
		frame_support::parameter_types! {
			pub BridgeLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(1234)));

			pub BridgeTable: sp_std::vec::Vec<(NetworkId, LocationFilter<InteriorMultiLocation>, MultiLocation, Option<MultiAsset>)> = sp_std::vec![
				(
					Kusama,
					LocationFilter::default()
						.add_equals(X1(Parachain(2000)))
						.add_equals(X1(Parachain(3000)))
						.add_equals(X1(Parachain(4000))),
					BridgeLocation::get(),
					None
				)
			];
		}

		let test_data = vec![
			(Polkadot, X1(Parachain(1000)), None),
			(Polkadot, X1(Parachain(2000)), None),
			(Polkadot, X1(Parachain(3000)), None),
			(Polkadot, X1(Parachain(4000)), None),
			(Polkadot, X1(Parachain(5000)), None),
			(Kusama, X1(Parachain(1000)), None),
			(Kusama, X1(Parachain(2000)), Some((BridgeLocation::get(), None))),
			(Kusama, X1(Parachain(3000)), Some((BridgeLocation::get(), None))),
			(Kusama, X1(Parachain(4000)), Some((BridgeLocation::get(), None))),
			(Kusama, X1(Parachain(5000)), None),
		];

		for (network, remote_location, expected_result) in test_data {
			assert_eq!(
				FilteredNetworkExportTable::<BridgeTable>::exporter_for(
					&network,
					&remote_location,
					&Xcm::default()
				),
				expected_result,
			)
		}
	}

	#[test]
	fn allow_unpaid_transacts_from_works() {
		#[derive(Encode, Decode)]
		enum RuntimeCall {
			CallA,
			CallB,
		}
		frame_support::match_types! {
			pub type AcceptOnlyFromSibling1002: impl Contains<MultiLocation> = {
				MultiLocation { parents: 1, interior: X1(Parachain(1002)) }
			};
			pub type AcceptOnlyRuntimeCallA: impl Contains<RuntimeCall> = { RuntimeCall::CallA };
		}

		fn transact(origin_kind: OriginKind, encoded_data: impl Encode) -> Instruction<()> {
			Transact {
				origin_kind,
				require_weight_at_most: Default::default(),
				call: encoded_data.encode().into(),
			}
		}

		type Barrier = AllowUnpaidTransactsFrom<
			RuntimeCall,
			AcceptOnlyRuntimeCallA,
			AcceptOnlyFromSibling1002,
		>;

		let test_data: Vec<(MultiLocation, Vec<Instruction<()>>, Result<(), ProcessMessageError>)> = vec![
			// success case
			(
				MultiLocation { parents: 1, interior: X1(Parachain(1002)) },
				vec![transact(OriginKind::Xcm, RuntimeCall::CallA)],
				Ok(()),
			),
			// invalid message - more instruction than just one `Transact`
			(
				MultiLocation { parents: 1, interior: X1(Parachain(1002)) },
				vec![transact(OriginKind::Xcm, RuntimeCall::CallA), ClearOrigin],
				Err(ProcessMessageError::BadFormat),
			),
			// invalid `OriginKind`
			(
				MultiLocation { parents: 1, interior: X1(Parachain(1002)) },
				vec![transact(OriginKind::Native, RuntimeCall::CallA)],
				Err(ProcessMessageError::BadFormat),
			),
			// unsupported call
			(
				MultiLocation { parents: 1, interior: X1(Parachain(1002)) },
				vec![transact(OriginKind::Xcm, RuntimeCall::CallB)],
				Err(ProcessMessageError::Unsupported),
			),
			// unsupported origin
			(
				MultiLocation { parents: 1, interior: X1(Parachain(2105)) },
				vec![transact(OriginKind::Xcm, RuntimeCall::CallA)],
				Err(ProcessMessageError::Unsupported),
			),
		];

		for (origin, mut xcm, expected_result) in test_data {
			assert_eq!(
				Barrier::should_execute(
					&origin,
					&mut xcm,
					Default::default(),
					&mut xcm_executor::traits::Properties {
						weight_credit: Default::default(),
						message_id: None,
					},
				),
				expected_result,
			)
		}
	}
}
