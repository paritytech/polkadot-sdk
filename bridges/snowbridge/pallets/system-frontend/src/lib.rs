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
use snowbridge_core::AssetMetadata;
use sp_core::H256;
use sp_io::hashing::blake2_256;
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;

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

		/// Origin check for XCM locations that can create agents
		type CreateAgentOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		/// Origin check for XCM locations that can create agents
		type RegisterTokenOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Location>;

		/// XCM message sender
		type XcmSender: SendXcm;

		/// To withdraw and deposit an asset.
		type AssetTransactor: TransactAsset;

		/// To charge XCM delivery fees
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;

		/// Fee asset for the execution cost on ethereum
		type EthereumLocation: Get<Location>;

		/// RemoteExecutionFee for the execution cost on bridge hub
		type RemoteExecutionFee: Get<Asset>;

		/// Location of bridge hub
		type BridgeHubLocation: Get<Location>;

		/// Universal location of this runtime.
		type UniversalLocation: Get<InteriorLocation>;

		type WeightInfo: WeightInfo;

		/// A set of helper functions for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A `CreateAgent` message was sent to Bridge Hub
		CreateAgent { location: Location, message_id: H256 },
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
		Send,
		/// Withdraw fee asset failure
		FundsUnavailable,
		/// Convert to reanchored location failure
		LocationConversionFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Call `create_agent` to instantiate a new agent contract representing `origin`.
		/// - `fee`: Fee in Ether paying for the execution cost on Ethreum
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::create_agent())]
		pub fn create_agent(origin: OriginFor<T>, fee: u128) -> DispatchResult {
			let origin_location = T::CreateAgentOrigin::ensure_origin(origin)?;

			// Burn Ether Fee for the cost on ethereum
			Self::burn_for_teleport(&origin_location, &(T::EthereumLocation::get(), fee).into())?;

			// Burn RemoteExecutionFee for the cost on bridge hub
			Self::burn_for_teleport(&origin_location, &T::RemoteExecutionFee::get())?;

			let reanchored_location = origin_location
				.clone()
				.reanchored(&T::BridgeHubLocation::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)?;

			let call = BridgeHubRuntime::EthereumSystem(EthereumSystemCall::CreateAgent {
				location: Box::new(VersionedLocation::from(reanchored_location.clone())),
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
				ExpectTransactStatus(MaybeErrorCode::Success),
			]
			.into();

			let message_id = Self::send(origin_location.clone(), xcm)?;

			Self::deposit_event(Event::<T>::CreateAgent { location: origin_location, message_id });
			Ok(())
		}

		/// Registers a Polkadot-native token as a wrapped ERC20 token on Ethereum.
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

			// Burn Ether Fee for the cost on ethereum
			Self::burn_for_teleport(&origin_location, &(T::EthereumLocation::get(), fee).into())?;

			// Burn RemoteExecutionFee for the cost on bridge hub
			Self::burn_for_teleport(&origin_location, &T::RemoteExecutionFee::get())?;

			let reanchored_asset_location = asset_location
				.clone()
				.reanchored(&T::BridgeHubLocation::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)?;

			let call = BridgeHubRuntime::EthereumSystem(EthereumSystemCall::RegisterToken {
				asset_id: Box::new(VersionedLocation::from(reanchored_asset_location.clone())),
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
				ExpectTransactStatus(MaybeErrorCode::Success),
			]
			.into();

			let message_id = Self::send(origin_location.clone(), xcm)?;

			Self::deposit_event(Event::<T>::RegisterToken { location: asset_location, message_id });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn send(origin: Location, xcm: Xcm<()>) -> Result<H256, Error<T>> {
			let bridgehub = T::BridgeHubLocation::get();
			let (message_id, price) =
				send_xcm::<T::XcmSender>(bridgehub, xcm).map_err(|_| Error::<T>::Send)?;

			T::XcmExecutor::charge_fees(origin, price).map_err(|_| Error::<T>::FundsUnavailable)?;

			Ok(message_id.into())
		}

		pub fn burn_for_teleport(origin: &Location, fee: &Asset) -> DispatchResult {
			let dummy_context =
				XcmContext { origin: None, message_id: Default::default(), topic: None };
			T::AssetTransactor::can_check_out(origin, fee, &dummy_context)
				.map_err(|_| Error::<T>::FundsUnavailable)?;
			T::AssetTransactor::check_out(origin, fee, &dummy_context);
			T::AssetTransactor::withdraw_asset(fee, origin, None)
				.map_err(|_| Error::<T>::FundsUnavailable)?;
			Ok(())
		}
	}
}
