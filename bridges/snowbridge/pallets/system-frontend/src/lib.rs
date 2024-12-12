// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Frontend for calling Snowbridge System Pallet on BridgeHub
//!
//! # Extrinsics
//!
//! * [`Call::create_agent`]: Create agent for any sovereign location from non-system parachain
//! * [`Call::register_token`]: Register a foreign token location from non-system parachain
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

use frame_support::{pallet_prelude::*, traits::EnsureOrigin};
use frame_system::pallet_prelude::*;
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;
use snowbridge_core::AssetMetadata;

#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum ControlCall {
	#[codec(index = 1)]
	CreateAgent { location: Box<VersionedLocation>, fee: u128 },
	#[codec(index = 2)]
	RegisterToken { asset_id: Box<VersionedLocation>, metadata: AssetMetadata, fee: u128 },
}

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum SnowbridgeControl {
	#[codec(index = 85)]
	Control(ControlCall),
}

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<O>
where
	O: OriginTrait,
{
	fn make_xcm_origin(location: Location) -> O;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin check for XCM locations that can create agents
		type CreateAgentOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		/// Origin check for XCM locations that can create agents
		type RegisterTokenOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		/// XCM message sender
		type XcmSender: SendXcm;

		/// To withdraw and deposit an asset.
		type AssetTransactor: TransactAsset;

		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;

		type WETH: Get<Location>;

		type DeliveryFee: Get<Asset>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An CreateAgent message was sent to BH
		CreateAgent { location: Location },
		/// Register Polkadot-native token was sent to BH
		RegisterToken {
			/// Location of Polkadot-native token
			location: Location,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		UnsupportedLocationVersion,
		OwnerCheck,
		Send,
		FundsUnavailable,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Call create_agent on BH to instantiate a new agent contract representing `origin`.
		/// - `origin`: Must be `Location` from a sibling parachain
		/// - `fee`: Fee in Ether
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::create_agent())]
		pub fn create_agent(origin: OriginFor<T>, fee: u128) -> DispatchResult {
			let origin_location = T::CreateAgentOrigin::ensure_origin(origin)?;

			Self::burn_fees(origin_location.clone(), fee)?;

			let call = SnowbridgeControl::Control(ControlCall::CreateAgent {
				location: Box::new(VersionedLocation::from(origin_location.clone())),
				fee,
			});

			let xcm: Xcm<()> = vec![
				ReceiveTeleportedAsset(T::DeliveryFee::get().into()),
				PayFees { asset: T::DeliveryFee::get() },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: call.encode().into(),
					fallback_max_weight: None,
				},
			]
			.into();

			Self::send(xcm)?;

			Self::deposit_event(Event::<T>::CreateAgent { location: origin_location.clone() });
			Ok(())
		}

		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
		/// - `origin`: Must be `Location` from a sibling parachain
		/// - `asset_id`: Location of the asset (should be starts from the dispatch origin)
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::register_token())]
		pub fn register_token(
			origin: OriginFor<T>,
			asset_id: Box<VersionedLocation>,
			metadata: AssetMetadata,
			fee: u128,
		) -> DispatchResult {
			let origin_location = T::RegisterTokenOrigin::ensure_origin(origin)?;

			let asset_location: Location =
				(*asset_id).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let mut checked = false;
			if asset_location.eq(&origin_location) || asset_location.starts_with(&origin_location) {
				checked = true
			}
			ensure!(checked, <Error<T>>::OwnerCheck);

			Self::burn_fees(origin_location.clone(), fee)?;

			let call = SnowbridgeControl::Control(ControlCall::RegisterToken {
				asset_id: Box::new(VersionedLocation::from(asset_location.clone())),
				metadata,
				fee,
			});

			let xcm: Xcm<()> = vec![
				ReceiveTeleportedAsset(T::DeliveryFee::get().into()),
				PayFees { asset: T::DeliveryFee::get() },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: call.encode().into(),
					fallback_max_weight: None,
				},
			]
			.into();

			Self::send(xcm)?;

			Self::deposit_event(Event::<T>::RegisterToken { location: asset_location });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn send(xcm: Xcm<()>) -> DispatchResult {
			let bridgehub = Location::new(1, [Parachain(1002)]);
			send_xcm::<T::XcmSender>(bridgehub, xcm).map_err(|_| Error::<T>::Send)?;
			Ok(())
		}
		pub fn burn_fees(origin_location: Location, fee: u128) -> DispatchResult {
			let ethereum_fee_asset = (T::WETH::get(), fee).into();
			T::AssetTransactor::withdraw_asset(&ethereum_fee_asset, &origin_location, None)
				.map_err(|_| Error::<T>::FundsUnavailable)?;
			T::AssetTransactor::withdraw_asset(&T::DeliveryFee::get(), &origin_location, None)
				.map_err(|_| Error::<T>::FundsUnavailable)?;
			Ok(())
		}
	}
}
