// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Frontend which will be deployed on AssetHub for calling the V2 system pallet
//! on BridgeHub.
//!
//! # Extrinsics
//!
//! * [`Call::create_agent`]: Create agent for any kind of sovereign location on Polkadot network.
//! * [`Call::register_token`]: Register Polkadot native asset as a wrapped ERC20 token on Ethereum.
#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

use frame_support::{pallet_prelude::*, traits::EnsureOriginWithArg};
use frame_system::pallet_prelude::*;
use snowbridge_core::AssetMetadata;
use sp_core::H256;
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;

pub const LOG_TARGET: &str = "snowbridge-system-frontend";

#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum EthereumSystemCall {
	#[codec(index = 0)]
	RegisterToken { asset_id: Box<VersionedLocation>, metadata: AssetMetadata },
}

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum BridgeHubRuntime {
	#[codec(index = 90)]
	EthereumSystem(EthereumSystemCall),
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

		/// Origin check for XCM locations that can register token
		type RegisterTokenOrigin: EnsureOriginWithArg<
			Self::RuntimeOrigin,
			Location,
			Success = Location,
		>;
		/// XCM message sender
		type XcmSender: SendXcm;

		/// To withdraw and deposit an asset.
		type AssetTransactor: TransactAsset;

		/// To charge XCM delivery fees
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;

		/// Fee asset for the execution cost on ethereum
		type EthereumLocation: Get<Location>;

		/// Location of bridge hub
		type BridgeHubLocation: Get<Location>;

		/// Universal location of this runtime.
		type UniversalLocation: Get<InteriorLocation>;

		/// InteriorLocation of this pallet.
		type PalletLocation: Get<InteriorLocation>;

		type WeightInfo: WeightInfo;

		/// A set of helper functions for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A message to register a Polkadot-native token was sent to Bridge Hub
		RegisterToken {
			/// Location of Polkadot-native token
			location: Location,
			message_id: H256,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Convert versioned location failure
		UnsupportedLocationVersion,
		/// Check location failure, should start from the dispatch origin as owner
		InvalidAssetOwner,
		/// Send xcm message failure
		SendFailure,
		/// Withdraw fee asset failure
		FeesNotMet,
		/// Convert to reanchored location failure
		LocationConversionFailed,
		/// The desired destination was unreachable, generally because there is a no way of routing
		/// to it.
		Unreachable,
	}

	impl<T: Config> From<SendError> for Error<T> {
		fn from(e: SendError) -> Self {
			match e {
				SendError::Fees => Error::<T>::FeesNotMet,
				SendError::NotApplicable => Error::<T>::Unreachable,
				_ => Error::<T>::SendFailure,
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
		/// - `asset_id`: Location of the asset (should starts from the dispatch origin)
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		/// - `fee`: Fee in Ether paying for the execution cost on Ethreum
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::register_token())]
		pub fn register_token(
			origin: OriginFor<T>,
			asset_id: Box<VersionedLocation>,
			metadata: AssetMetadata,
		) -> DispatchResult {
			let asset_location: Location =
				(*asset_id).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;

			let origin_location = T::RegisterTokenOrigin::ensure_origin(origin, &asset_location)?;

			let reanchored_asset_location = Self::reanchor(&asset_location)?;

			let call = BridgeHubRuntime::EthereumSystem(EthereumSystemCall::RegisterToken {
				asset_id: Box::new(VersionedLocation::from(reanchored_asset_location.clone())),
				metadata,
			});

			let message_id = Self::send(origin_location.clone(), Self::build_xcm(&call))?;

			Self::deposit_event(Event::<T>::RegisterToken { location: asset_location, message_id });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn send(origin: Location, xcm: Xcm<()>) -> Result<H256, Error<T>> {
			let (message_id, price) =
				send_xcm::<T::XcmSender>(T::BridgeHubLocation::get(), xcm.clone()).map_err(
					|err| {
						tracing::error!(target: LOG_TARGET, ?err, ?xcm, "XCM send failed with error");
						Error::<T>::from(err)
					},
				)?;
			T::XcmExecutor::charge_fees(origin, price).map_err(|_| Error::<T>::FeesNotMet)?;
			Ok(message_id.into())
		}

		fn burn_for_teleport(origin: &Location, fee: &Asset) -> DispatchResult {
			let dummy_context =
				XcmContext { origin: None, message_id: Default::default(), topic: None };
			T::AssetTransactor::can_check_out(origin, fee, &dummy_context)
				.map_err(|_| Error::<T>::FeesNotMet)?;
			T::AssetTransactor::check_out(origin, fee, &dummy_context);
			T::AssetTransactor::withdraw_asset(fee, origin, None)
				.map_err(|_| Error::<T>::FeesNotMet)?;
			Ok(())
		}

		fn build_xcm(call: &impl Encode) -> Xcm<()> {
			Xcm(vec![
				DescendOrigin(T::PalletLocation::get()),
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: call.encode().into(),
					fallback_max_weight: None,
				},
			])
		}
		/// Reanchors `location` relative to BridgeHub.
		fn reanchor(location: &Location) -> Result<Location, Error<T>> {
			location
				.clone()
				.reanchored(&T::BridgeHubLocation::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)
		}
	}
}
