// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Inbound Queue
//!
//! # Overview
//!
//! Receives messages emitted by the Gateway contract on Ethereum, whereupon they are verified,
//! translated to XCM, and finally sent to AssetHub for further processing.
//!
//! Message relayers are rewarded in wrapped Ether that is included within the message. This
//! wrapped Ether is derived from Ether that the message origin has locked up on Ethereum.
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

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod types;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test;

use codec::{Decode, Encode};
use frame_support::{
	traits::{
		fungible::{Inspect, Mutate},
		tokens::Balance,
	},
	weights::WeightToFee,
	PalletError,
};
use frame_system::ensure_signed;
use scale_info::TypeInfo;
use snowbridge_core::{
	sparse_bitmap::SparseBitmap,
	BasicOperatingMode,
};
use snowbridge_inbound_queue_primitives::{
	VerificationError, Verifier, EventProof,
	v2::{Message, ConvertMessage, ConvertMessageError}
};
use sp_core::H160;
use types::Nonce;
pub use weights::WeightInfo;
use xcm::prelude::{send_xcm, Junction::*, Location, SendError as XcmpSendError, SendXcm, *};

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
	use frame_system::pallet_prelude::*;
	use sp_std::prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[cfg(feature = "runtime-benchmarks")]
	pub trait BenchmarkHelper<T> {
		fn initialize_storage(beacon_header: BeaconHeader, block_roots_root: H256);
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The verifier for inbound messages from Ethereum.
		type Verifier: Verifier;
		/// XCM message sender.
		type XcmSender: SendXcm;
		/// Address of the Gateway contract.
		#[pallet::constant]
		type GatewayAddress: Get<H160>;
		type WeightInfo: WeightInfo;
		/// Convert a weight value into deductible balance type.
		type WeightToFee: WeightToFee<Balance = BalanceOf<Self>>;
		/// AssetHub parachain ID.
		type AssetHubParaId: Get<u32>;
		/// Convert a command from Ethereum to an XCM message.
		type MessageConverter: ConvertMessage;
		/// Used to burn fees from the origin account (the relayer), which will be teleported to AH.
		type Token: Mutate<Self::AccountId> + Inspect<Self::AccountId>;
		/// Used for the dry run API implementation.
		type Balance: Balance + From<u128>;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper: BenchmarkHelper<Self>;
	}

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
		/// Fee provided is invalid.
		InvalidFee,
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

	impl<T: Config> From<XcmpSendError> for Error<T> {
		fn from(e: XcmpSendError) -> Self {
			match e {
				XcmpSendError::NotApplicable => Error::<T>::Send(SendError::NotApplicable),
				XcmpSendError::Unroutable => Error::<T>::Send(SendError::NotRoutable),
				XcmpSendError::Transport(_) => Error::<T>::Send(SendError::Transport),
				XcmpSendError::DestinationUnsupported => Error::<T>::Send(SendError::DestinationUnsupported),
				XcmpSendError::ExceedsMaxMessageSize => Error::<T>::Send(SendError::ExceedsMaxMessageSize),
				XcmpSendError::MissingArgument => Error::<T>::Send(SendError::MissingArgument),
				XcmpSendError::Fees => Error::<T>::Send(SendError::Fees),
			}
		}
	}

	/// The nonce of the message been processed or not
	#[pallet::storage]
	pub type NonceBitmap<T: Config> = StorageMap<_, Twox64Concat, u128, u128, ValueQuery>;

	/// The current operating mode of the pallet.
	#[pallet::storage]
	pub type OperatingMode<T: Config> = StorageValue<_, BasicOperatingMode, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit an inbound message originating from the Gateway contract on Ethereum
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::submit())]
		pub fn submit(origin: OriginFor<T>, event: Box<EventProof>) -> DispatchResult {
			let who = ensure_signed(origin.clone())?;
			ensure!(!OperatingMode::<T>::get().is_halted(), Error::<T>::Halted);

			// submit message for verification
			T::Verifier::verify(&event.event_log, &event.proof)
				.map_err(|e| Error::<T>::Verification(e))?;

			// Decode event log into an Envelope
			let message =
				Message::try_from(&event.event_log).map_err(|_| Error::<T>::InvalidEnvelope)?;

			// Verify that the message was submitted from the known Gateway contract
			ensure!(T::GatewayAddress::get() == message.gateway, Error::<T>::InvalidGateway);

			// Verify the message has not been processed
			ensure!(!Nonce::<T>::get(message.nonce.into()), Error::<T>::InvalidNonce);

			let origin_account_location = Self::account_to_location(who)?;

			let (xcm, _relayer_reward) =
				Self::do_convert(message.clone(), origin_account_location.clone())?;

			// TODO: Deposit `_relayer_reward` (ether) to RewardLedger pallet which should cover all of:
			// T::RewardLedger::deposit(who, relayer_reward.into())?;
			// a. The submit extrinsic cost on BH
			// b. The delivery cost to AH
			// c. The execution cost on AH
			// d. The execution cost on destination chain(if any)
			// e. The reward

			Nonce::<T>::set(message.nonce.into());

			// Attempt to forward XCM to AH
			let message_id = Self::send_xcm(xcm, T::AssetHubParaId::get())?;
			Self::deposit_event(Event::MessageReceived { nonce: message.nonce, message_id });

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

		pub fn send_xcm(xcm: Xcm<()>, dest_para_id: u32) -> Result<XcmHash, Error<T>> {
			let dest = Location::new(1, [Parachain(dest_para_id)]);
			let (message_id, _) = send_xcm::<T::XcmSender>(dest, xcm).map_err(Error::<T>::from)?;
			Ok(message_id)
		}

		pub fn do_convert(
			message: Message,
			origin_account_location: Location,
		) -> Result<(Xcm<()>, u128), Error<T>> {
			Ok(T::MessageConverter::convert(message, origin_account_location)
				.map_err(|e| Error::<T>::ConvertMessage(e))?)
		}
	}
}
