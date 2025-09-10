// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//!
//! System frontend pallet that acts as the user-facing control-plane for Snowbridge.
//!
//! Some operations are delegated to a backend pallet installed on a remote parachain.
//!
//! # Extrinsics
//!
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

pub mod backend_weights;
pub use backend_weights::*;

use frame_support::{pallet_prelude::*, traits::EnsureOriginWithArg};
use frame_system::pallet_prelude::*;
use pallet_asset_conversion::Swap;
use snowbridge_core::{
	burn_for_teleport, operating_mode::ExportPausedQuery, reward::MessageId, AssetMetadata,
	BasicOperatingMode as OperatingMode,
};
use sp_std::prelude::*;
use xcm::{
	latest::{validate_send, XcmHash},
	prelude::*,
};
use xcm_executor::traits::{FeeManager, FeeReason, TransactAsset};

#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::OriginTrait;

pub use pallet::*;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

pub const LOG_TARGET: &str = "snowbridge-system-frontend";

/// Call indices within BridgeHub runtime for dispatchables within `snowbridge-pallet-system-v2`
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum BridgeHubRuntime<T: frame_system::Config> {
	#[codec(index = 90)]
	EthereumSystem(EthereumSystemCall<T>),
}

/// Call indices for dispatchables within `snowbridge-pallet-system-v2`
#[derive(Encode, Decode, Debug, PartialEq, Clone, TypeInfo)]
pub enum EthereumSystemCall<T: frame_system::Config> {
	#[codec(index = 2)]
	RegisterToken {
		sender: Box<VersionedLocation>,
		asset_id: Box<VersionedLocation>,
		metadata: AssetMetadata,
		amount: u128,
	},
	#[codec(index = 3)]
	AddTip { sender: AccountIdOf<T>, message_id: MessageId, amount: u128 },
}

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<O, AccountId>
where
	O: OriginTrait,
{
	fn make_xcm_origin(location: Location) -> O;
	fn initialize_storage(asset_location: Location, asset_owner: Location);
	fn setup_pools(caller: AccountId, asset: Location);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use xcm_executor::traits::ConvertLocation;
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[allow(deprecated)]
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
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall> + FeeManager;

		/// Fee asset for the execution cost on ethereum
		type EthereumLocation: Get<Location>;
		/// To swap the provided tip asset for
		type Swap: Swap<Self::AccountId, AssetKind = Location, Balance = u128>;

		/// Location of bridge hub
		type BridgeHubLocation: Get<Location>;

		/// Universal location of this runtime.
		type UniversalLocation: Get<InteriorLocation>;

		/// InteriorLocation of this pallet.
		type PalletLocation: Get<InteriorLocation>;

		type AccountIdConverter: ConvertLocation<Self::AccountId>;

		/// Weights for dispatching XCM to backend implementation of `register_token`
		type BackendWeightInfo: BackendWeightInfo;

		/// Weights for pallet dispatchables
		type WeightInfo: WeightInfo;

		/// A set of helper functions for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self::RuntimeOrigin, Self::AccountId>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An XCM was sent
		MessageSent {
			origin: Location,
			destination: Location,
			message: Xcm<()>,
			message_id: XcmHash,
		},
		/// Set OperatingMode
		ExportOperatingModeChanged { mode: OperatingMode },
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
		/// Message export is halted
		Halted,
		/// The desired destination was unreachable, generally because there is a no way of routing
		/// to it.
		Unreachable,
		/// The asset provided for the tip is unsupported.
		UnsupportedAsset,
		/// Unable to withdraw asset.
		WithdrawError,
		/// Account could not be converted to a location.
		InvalidAccount,
		/// Provided tip asset could not be swapped for ether.
		SwapError,
		/// Ether could not be burned.
		BurnError,
		/// The tip provided is zero.
		TipAmountZero,
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

	/// The current operating mode for exporting to Ethereum.
	#[pallet::storage]
	#[pallet::getter(fn export_operating_mode)]
	pub type ExportOperatingMode<T: Config> = StorageValue<_, OperatingMode, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		<T as frame_system::Config>::AccountId: Into<Location>,
	{
		/// Set the operating mode for exporting messages to Ethereum.
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(origin: OriginFor<T>, mode: OperatingMode) -> DispatchResult {
			ensure_root(origin)?;
			ExportOperatingMode::<T>::put(mode);
			Self::deposit_event(Event::ExportOperatingModeChanged { mode });
			Ok(())
		}

		/// Initiates the registration for a Polkadot-native token as a wrapped ERC20 token on
		/// Ethereum.
		/// - `asset_id`: Location of the asset
		/// - `metadata`: Metadata to include in the instantiated ERC20 contract on Ethereum
		///
		/// All origins are allowed, however `asset_id` must be a location nested within the origin
		/// consensus system.
		#[pallet::call_index(1)]
		#[pallet::weight(
			T::WeightInfo::register_token()
				.saturating_add(T::BackendWeightInfo::transact_register_token())
				.saturating_add(T::BackendWeightInfo::do_process_message())
				.saturating_add(T::BackendWeightInfo::commit_single())
				.saturating_add(T::BackendWeightInfo::submit_delivery_receipt())
		)]
		pub fn register_token(
			origin: OriginFor<T>,
			asset_id: Box<VersionedLocation>,
			metadata: AssetMetadata,
			fee_asset: Asset,
		) -> DispatchResult {
			ensure!(!Self::export_operating_mode().is_halted(), Error::<T>::Halted);

			let asset_location: Location =
				(*asset_id).try_into().map_err(|_| Error::<T>::UnsupportedLocationVersion)?;
			let origin_location = T::RegisterTokenOrigin::ensure_origin(origin, &asset_location)?;

			let ether_gained = if origin_location.is_here() {
				// Root origin/location does not pay any fees/tip.
				0
			} else {
				Self::swap_fee_asset_and_burn(origin_location.clone(), fee_asset)?
			};

			let call = Self::build_register_token_call(
				origin_location.clone(),
				asset_location,
				metadata,
				ether_gained,
			)?;

			Self::send_transact_call(origin_location, call)
		}

		/// Add an additional relayer tip for a committed message identified by `message_id`.
		/// The tip asset will be swapped for ether.
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::add_tip()
				.saturating_add(T::BackendWeightInfo::transact_add_tip())
		)]
		pub fn add_tip(origin: OriginFor<T>, message_id: MessageId, asset: Asset) -> DispatchResult
		where
			<T as frame_system::Config>::AccountId: Into<Location>,
		{
			let who = ensure_signed(origin)?;

			let ether_gained = Self::swap_fee_asset_and_burn(who.clone().into(), asset)?;

			// Send the tip details to BH to be allocated to the reward in the Inbound/Outbound
			// pallet
			let call = Self::build_add_tip_call(who.clone(), message_id.clone(), ether_gained);
			Self::send_transact_call(who.into(), call)
		}
	}

	impl<T: Config> Pallet<T> {
		fn send_xcm(origin: Location, dest: Location, xcm: Xcm<()>) -> Result<XcmHash, SendError> {
			let is_waived =
				<T::XcmExecutor as FeeManager>::is_waived(Some(&origin), FeeReason::ChargeFees);
			let (ticket, price) = validate_send::<T::XcmSender>(dest, xcm.clone())?;
			if !is_waived {
				T::XcmExecutor::charge_fees(origin, price).map_err(|_| SendError::Fees)?;
			}
			T::XcmSender::deliver(ticket)
		}

		/// Swaps a specified tip asset to Ether and then burns the resulting ether for
		/// teleportation. Returns the amount of Ether gained if successful, or a DispatchError if
		/// any step fails.
		fn swap_and_burn(
			origin: Location,
			tip_asset_location: Location,
			ether_location: Location,
			tip_amount: u128,
		) -> Result<u128, DispatchError> {
			// Swap tip asset to ether
			let swap_path = vec![tip_asset_location.clone(), ether_location.clone()];
			let who = T::AccountIdConverter::convert_location(&origin)
				.ok_or(Error::<T>::LocationConversionFailed)?;

			let ether_gained = T::Swap::swap_exact_tokens_for_tokens(
				who.clone(),
				swap_path,
				tip_amount,
				None, // No minimum amount required
				who,
				true,
			)?;

			// Burn the ether
			let ether_asset = Asset::from((ether_location.clone(), ether_gained));

			burn_for_teleport::<T::AssetTransactor>(&origin, &ether_asset)
				.map_err(|_| Error::<T>::BurnError)?;

			Ok(ether_gained)
		}

		// Build the call to dispatch the `EthereumSystem::register_token` extrinsic on BH
		fn build_register_token_call(
			sender: Location,
			asset: Location,
			metadata: AssetMetadata,
			amount: u128,
		) -> Result<BridgeHubRuntime<T>, Error<T>> {
			// reanchor locations relative to BH
			let sender = Self::reanchored(sender)?;
			let asset = Self::reanchored(asset)?;

			let call = BridgeHubRuntime::EthereumSystem(EthereumSystemCall::RegisterToken {
				sender: Box::new(VersionedLocation::from(sender)),
				asset_id: Box::new(VersionedLocation::from(asset)),
				metadata,
				amount,
			});

			Ok(call)
		}

		// Build the call to dispatch the `EthereumSystem::add_tip` extrinsic on BH
		fn build_add_tip_call(
			sender: AccountIdOf<T>,
			message_id: MessageId,
			amount: u128,
		) -> BridgeHubRuntime<T> {
			BridgeHubRuntime::EthereumSystem(EthereumSystemCall::AddTip {
				sender,
				message_id,
				amount,
			})
		}

		fn build_remote_xcm(call: &impl Encode) -> Xcm<()> {
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
		fn reanchored(location: Location) -> Result<Location, Error<T>> {
			location
				.reanchored(&T::BridgeHubLocation::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::LocationConversionFailed)
		}

		fn swap_fee_asset_and_burn(
			origin: Location,
			fee_asset: Asset,
		) -> Result<u128, DispatchError> {
			let ether_location = T::EthereumLocation::get();
			let (fee_asset_location, fee_amount) = match fee_asset {
				Asset { id: AssetId(ref loc), fun: Fungible(amount) } => (loc, amount),
				_ => {
					tracing::debug!(target: LOG_TARGET, ?fee_asset, "error matching fee asset");
					return Err(Error::<T>::UnsupportedAsset.into())
				},
			};
			if fee_amount == 0 {
				return Ok(0)
			}

			let ether_gained = if *fee_asset_location != ether_location {
				Self::swap_and_burn(
					origin.clone(),
					fee_asset_location.clone(),
					ether_location,
					fee_amount,
				)
				.inspect_err(|&e| {
					tracing::debug!(target: LOG_TARGET, ?e, "error swapping asset");
				})?
			} else {
				burn_for_teleport::<T::AssetTransactor>(&origin, &fee_asset)
					.map_err(|_| Error::<T>::BurnError)?;
				fee_amount
			};
			Ok(ether_gained)
		}

		fn send_transact_call(
			origin_location: Location,
			call: BridgeHubRuntime<T>,
		) -> DispatchResult {
			let dest = T::BridgeHubLocation::get();
			let remote_xcm = Self::build_remote_xcm(&call);
			let message_id = Self::send_xcm(origin_location, dest.clone(), remote_xcm.clone())
				.map_err(|error| Error::<T>::from(error))?;

			Self::deposit_event(Event::<T>::MessageSent {
				origin: T::PalletLocation::get().into(),
				destination: dest,
				message: remote_xcm,
				message_id,
			});

			Ok(())
		}
	}

	impl<T: Config> ExportPausedQuery for Pallet<T> {
		fn is_paused() -> bool {
			Self::export_operating_mode().is_halted()
		}
	}
}
