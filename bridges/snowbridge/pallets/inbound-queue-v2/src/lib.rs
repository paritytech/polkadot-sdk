// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Inbound Queue
//!
//! # Overview
//!
//! Receives messages emitted by the Gateway contract on Ethereum, whereupon they are verified,
//! translated to XCM, and finally sent to their final destination parachain.
//!
//! The message relayers are rewarded using native currency from the sovereign account of the
//! destination parachain.
//!
//! # Extrinsics
//!
//! ## Governance
//!
//! * [`Call::set_operating_mode`]: Set the operating mode of the pallet. Can be used to disable
//!   processing of inbound messages.
//!
//! ## Message Submission
//!
//! * [`Call::submit`]: Submit a message for verification and dispatch the final destination
//!   parachain.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
pub mod api;
mod envelope;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod types;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

use codec::{Decode, DecodeAll, Encode};
use envelope::Envelope;
use frame_support::{
	traits::fungible::{Inspect, Mutate},
	PalletError,
};
use frame_system::{ensure_signed, pallet_prelude::*};
use scale_info::TypeInfo;
use sp_core::H160;
use sp_std::vec;
use types::Nonce;
use alloc::boxed::Box;
use xcm::prelude::{Junction::*, Location, *};

use snowbridge_core::{
	fees::burn_fees,
	inbound::{Message, VerificationError, Verifier},
	sparse_bitmap::SparseBitmap,
	BasicOperatingMode,
};
use snowbridge_router_primitives::inbound::v2::{
	ConvertMessage, ConvertMessageError, Message as MessageV2,
};
pub use weights::WeightInfo;
use xcm::{VersionedLocation, VersionedXcm};
use xcm_builder::SendController;
use xcm_executor::traits::TransactAsset;

#[cfg(feature = "runtime-benchmarks")]
use snowbridge_beacon_primitives::BeaconHeader;

pub use pallet::*;

pub const LOG_TARGET: &str = "snowbridge-inbound-queue:v2";

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type BalanceOf<T> =
	<<T as pallet::Config>::Token as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[cfg(feature = "runtime-benchmarks")]
	pub trait BenchmarkHelper<T> {
		fn initialize_storage(beacon_header: BeaconHeader, block_roots_root: H256);
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The verifier for inbound messages from Ethereum
		type Verifier: Verifier;

		/// XCM message sender
		type XcmSender: SendController<<Self as frame_system::Config>::RuntimeOrigin>;
		/// Address of the Gateway contract
		#[pallet::constant]
		type GatewayAddress: Get<H160>;
		type WeightInfo: WeightInfo;
		/// AssetHub parachain ID
		type AssetHubParaId: Get<u32>;
		type MessageConverter: ConvertMessage;
		type XcmPrologueFee: Get<BalanceOf<Self>>;
		type Token: Mutate<Self::AccountId> + Inspect<Self::AccountId>;
		type AssetTransactor: TransactAsset;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A message was received from Ethereum
		MessageReceived {
			/// The message nonce
			nonce: u64,
			/// ID of the XCM message which was forwarded to the final destination parachain
			message_id: [u8; 32],
		},
		/// Set OperatingMode
		OperatingModeChanged { mode: BasicOperatingMode },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Message came from an invalid outbound channel on the Ethereum side.
		InvalidGateway,
		/// Account could not be converted to bytes
		InvalidAccount,
		/// Message has an invalid envelope.
		InvalidEnvelope,
		/// Message has an unexpected nonce.
		InvalidNonce,
		/// Message has an invalid payload.
		InvalidPayload,
		/// Message channel is invalid
		InvalidChannel,
		/// The max nonce for the type has been reached
		MaxNonceReached,
		/// Cannot convert location
		InvalidAccountConversion,
		/// Pallet is halted
		Halted,
		/// Message verification error,
		Verification(VerificationError),
		/// XCMP send failure
		Send(SendError),
		/// Message conversion error
		ConvertMessage(ConvertMessageError),
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, PalletError)]
	pub enum SendError {
		NotApplicable,
		NotRoutable,
		Transport,
		DestinationUnsupported,
		ExceedsMaxMessageSize,
		MissingArgument,
		Fees,
	}

	/// The nonce of the message been processed or not
	#[pallet::storage]
	pub type NonceBitmap<T: Config> = StorageMap<_, Twox64Concat, u128, u128, ValueQuery>;

	/// The current operating mode of the pallet.
	#[pallet::storage]
	#[pallet::getter(fn operating_mode)]
	pub type OperatingMode<T: Config> = StorageValue<_, BasicOperatingMode, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit an inbound message originating from the Gateway contract on Ethereum
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::submit())]
		pub fn submit(origin: OriginFor<T>, message: Message) -> DispatchResult {
			let who = ensure_signed(origin.clone())?;
			ensure!(!Self::operating_mode().is_halted(), Error::<T>::Halted);

			// submit message to verifier for verification
			T::Verifier::verify(&message.event_log, &message.proof)
				.map_err(|e| Error::<T>::Verification(e))?;

			// Decode event log into an Envelope
			let envelope =
				Envelope::try_from(&message.event_log).map_err(|_| Error::<T>::InvalidEnvelope)?;

			// Verify that the message was submitted from the known Gateway contract
			ensure!(T::GatewayAddress::get() == envelope.gateway, Error::<T>::InvalidGateway);

			// Verify the message has not been processed
			ensure!(!Nonce::<T>::get(envelope.nonce.into()), Error::<T>::InvalidNonce);

			// Decode payload into `MessageV2`
			let message = MessageV2::decode_all(&mut envelope.payload.as_ref())
				.map_err(|_| Error::<T>::InvalidPayload)?;

			let xcm =
				T::MessageConverter::convert(message).map_err(|e| Error::<T>::ConvertMessage(e))?;

			// Burn the required fees for the static XCM message part
			burn_fees::<T::AssetTransactor, BalanceOf<T>>(
				Self::account_to_location(who)?,
				T::XcmPrologueFee::get(),
			)?;

			// Todo: Deposit fee(in Ether) to RewardLeger which should cover all of:
			// T::RewardLeger::deposit(who, envelope.fee.into())?;
			// a. The submit extrinsic cost on BH
			// b. The delivery cost to AH
			// c. The execution cost on AH
			// d. The execution cost on destination chain(if any)
			// e. The reward

			// Attempt to forward XCM to AH

			let message_id = Self::send_xcm(origin, xcm, T::AssetHubParaId::get())?;
			Self::deposit_event(Event::MessageReceived { nonce: envelope.nonce, message_id });

			// Set nonce flag to true
			Nonce::<T>::set(envelope.nonce.into());

			Ok(())
		}

		/// Halt or resume all pallet operations. May only be called by root.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 1), DispatchClass::Operational))]
		pub fn set_operating_mode(
			origin: OriginFor<T>,
			mode: BasicOperatingMode,
		) -> DispatchResult {
			ensure_root(origin)?;
			OperatingMode::<T>::set(mode);
			Self::deposit_event(Event::OperatingModeChanged { mode });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn account_to_location(account: AccountIdOf<T>) -> Result<Location, Error<T>> {
			let account_bytes: [u8; 32] =
				account.encode().try_into().map_err(|_| Error::<T>::InvalidAccount)?;
			Ok(Location::new(0, [AccountId32 { network: None, id: account_bytes }]))
		}

		pub fn send_xcm(origin: OriginFor<T>, xcm: Xcm<()>, dest_para_id: u32) -> Result<XcmHash, DispatchError> {
			let versioned_dest = Box::new(VersionedLocation::V5(Location::new(
				1,
				[Parachain(dest_para_id)],
			)));
			let versioned_xcm = Box::new(VersionedXcm::V5(xcm));
			Ok(T::XcmSender::send(origin, versioned_dest, versioned_xcm)?)
		}

		pub fn do_convert(message: MessageV2) -> Result<Xcm<()>, Error<T>> {
			Ok(T::MessageConverter::convert(message, T::XcmPrologueFee::get().into()).map_err(|e| Error::<T>::ConvertMessage(e))?)
		}
	}
}
