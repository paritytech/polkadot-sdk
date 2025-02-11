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
pub enum EthereumSystemCall {
	#[codec(index = 1)]
	CreateAgent { location: Box<VersionedLocation>, fee: u128 },
	#[codec(index = 2)]
	RegisterToken { asset_id: Box<VersionedLocation>, metadata: AssetMetadata, fee: u128 },
}

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum BridgeHubRuntime {
	#[codec(index = 90)]
	Control(EthereumSystemCall),
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

		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;

		/// Fee asset for the execution cost on ethereum
		type FeeAsset: Get<Location>;

		/// RemoteExecutionFee for the execution cost on bridge hub
		type RemoteExecutionFee: Get<Asset>;

		/// Location of bridge hub
		type BridgeHub: Get<Location>;

		type WeightInfo: WeightInfo;

		/// A set of helper functions for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A `CreateAgent` message was sent to Bridge Hub
		CreateAgent { location: Location },
		/// A message to register a Polkadot-native token was sent to Bridge Hub
		RegisterToken {
			/// Location of Polkadot-native token
			location: Location,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Convert versioned location failure
		UnsupportedLocationVersion,
		/// Check location failure, should start from the dispatch origin as owner
		OwnerCheck,
		/// Send xcm message failure
		Send,
		/// Withdraw fee asset failure
		FundsUnavailable,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Call `create_agent` to instantiate a new agent contract representing `origin`.
		/// - `origin`: Can be any sovereign `Location`
		/// - `fee`: Fee in Ether paying for the execution cost on Ethreum
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::create_agent())]
		pub fn create_agent(origin: OriginFor<T>, fee: u128) -> DispatchResult {
			let origin_location = T::CreateAgentOrigin::ensure_origin(origin)?;

			// Burn Ether Fee for the cost on ethereum
			T::AssetTransactor::withdraw_asset(
				&(T::FeeAsset::get(), fee).into(),
				&origin_location,
				None,
			)
			.map_err(|_| Error::<T>::FundsUnavailable)?;

			// Burn RemoteExecutionFee for the cost on bridge hub
			T::AssetTransactor::withdraw_asset(
				&T::RemoteExecutionFee::get(),
				&origin_location,
				None,
			)
			.map_err(|_| Error::<T>::FundsUnavailable)?;

			let call = BridgeHubRuntime::Control(EthereumSystemCall::CreateAgent {
				location: Box::new(VersionedLocation::from(origin_location.clone())),
				fee,
			});

			let xcm: Xcm<()> = vec![
				// Burn some DOT fees from the origin on AH and teleport to BH which pays for
				// the execution of Transacts on BH.
				ReceiveTeleportedAsset(T::RemoteExecutionFee::get().into()),
				PayFees { asset: T::RemoteExecutionFee::get() },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: call.encode().into(),
					fallback_max_weight: None,
				},
			]
			.into();

			Self::send(origin_location.clone(), xcm)?;

			Self::deposit_event(Event::<T>::CreateAgent { location: origin_location });
			Ok(())
		}

		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
		/// - `origin`: Must be `Location` from a sibling parachain
		/// - `asset_id`: Location of the asset (should starts from the dispatch origin)
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		/// - `fee`: Fee in Ether paying for the execution cost on Ethreum
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

			// Burn Ether Fee for the cost on ethereum
			T::AssetTransactor::withdraw_asset(
				&(T::FeeAsset::get(), fee).into(),
				&origin_location,
				None,
			)
			.map_err(|_| Error::<T>::FundsUnavailable)?;

			// Burn RemoteExecutionFee for the cost on bridge hub
			T::AssetTransactor::withdraw_asset(
				&T::RemoteExecutionFee::get(),
				&origin_location,
				None,
			)
			.map_err(|_| Error::<T>::FundsUnavailable)?;

			let call = BridgeHubRuntime::Control(EthereumSystemCall::RegisterToken {
				asset_id: Box::new(VersionedLocation::from(asset_location.clone())),
				metadata,
				fee,
			});

			let xcm: Xcm<()> = vec![
				ReceiveTeleportedAsset(T::RemoteExecutionFee::get().into()),
				PayFees { asset: T::RemoteExecutionFee::get() },
				Transact {
					origin_kind: OriginKind::Xcm,
					call: call.encode().into(),
					fallback_max_weight: None,
				},
			]
			.into();

			Self::send(origin_location.clone(), xcm)?;

			Self::deposit_event(Event::<T>::RegisterToken { location: asset_location });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn send(origin: Location, xcm: Xcm<()>) -> DispatchResult {
			let bridgehub = T::BridgeHub::get();
			let (_, price) =
				send_xcm::<T::XcmSender>(bridgehub, xcm).map_err(|_| Error::<T>::Send)?;
			T::XcmExecutor::charge_fees(origin, price).map_err(|_| Error::<T>::FundsUnavailable)?;
			Ok(())
		}
	}
}
