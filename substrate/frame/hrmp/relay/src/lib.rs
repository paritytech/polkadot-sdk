// This file is part of Substrate.

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

//! # HRMP pallet (relay-chain side)
//!
//! Serves a parachain's own HRMP requests on the relay chain, and carries HRMP deposits to and from
//! the parachain that holds them.
//!
//! - [`Call::relay_request`] takes a [`ParaRequest`] from a parachain and dispatches it into the
//!   relay chain's HRMP as that para, through [`Config::Hrmp`].
//! - [`Pallet::hold`] asks the deposit-holding parachain to hold a deposit, and [`Call::receive`]
//!   takes its answer to [`Config::OnDepositHeld`].
//! - [`Call::receive`] also serves the calls users make on the deposit-holding parachain:
//!   `poke_channel_deposits` and `establish_system_channel`.
//! - [`Pallet::release`] queues a release. Queued releases are sent before the next hold and at the
//!   start of every block, so the parachain receives holds and releases in the order they were
//!   made.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::traits::EnsureOrigin;
use hrmp_primitives::{
	Balance, DepositKey, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1,
	OnDepositHeld, ParaId, ParaRequest, ParaRequestV1, RelayHrmp,
};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Used to send an XCM `Transact` to the HRMP pallet on the deposit-holding parachain.
pub trait SendToPara {
	/// Send `message` to the parachain.
	///
	/// `Err(())` means the message could not be handed to the transport.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToPara) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
}

/// Decides whether a parachain's request is served.
pub trait AdmitRequest {
	/// Whether to serve a request from `para` now. May record that it was served.
	fn admit(para: ParaId) -> bool;
}

impl AdmitRequest for () {
	fn admit(_: ParaId) -> bool {
		true
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The parachain that holds the deposits.
		type ParaOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Any parachain acting as itself.
		type ParachainOrigin: EnsureOrigin<Self::RuntimeOrigin, Success: Into<ParaId>>;

		/// Sends messages to the parachain that holds the deposits.
		type SendToPara: SendToPara;

		/// The relay chain's HRMP.
		type Hrmp: RelayHrmp;

		/// Receives the answer to a hold.
		type OnDepositHeld: OnDepositHeld;

		/// Decides whether a parachain's request is served.
		type AdmitRequest: AdmitRequest;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Releases not yet sent, oldest first: the deposit, and how much of it (`None` for all).
	#[pallet::storage]
	#[pallet::unbounded]
	pub type PendingReleases<T: Config> =
		StorageValue<_, Vec<(DepositKey, Option<Balance>)>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A parachain's request was served.
		RequestServed { para: ParaId },
		/// The parachain was asked to hold a deposit.
		HoldSent { key: DepositKey, amount: Balance },
		/// The parachain answered a hold.
		HoldAnswered { key: DepositKey, held: bool },
		/// The parachain was asked to release a deposit, or `amount` of it.
		ReleaseSent { key: DepositKey, amount: Option<Balance> },
		/// A release could not be handed to the transport, and was dropped.
		ReleaseFailed { key: DepositKey, amount: Option<Balance> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The request is not served, for now.
		RequestRefused,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let sent = Self::flush_releases();
			T::WeightInfo::flush_releases(sent)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Take a message from the parachain that holds the deposits.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::receive())]
		pub fn receive(origin: OriginFor<T>, message: MessageToRelay) -> DispatchResult {
			T::ParaOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToRelay::V1(MessageToRelayV1::HoldResult { key, held }) => {
					T::OnDepositHeld::on_deposit_held(key, held);
					Self::deposit_event(Event::HoldAnswered { key, held });
				},
				MessageToRelay::V1(MessageToRelayV1::PokeChannelDeposits { channel }) => {
					T::Hrmp::poke_channel_deposits(channel)?
				},
				MessageToRelay::V1(MessageToRelayV1::EstablishSystemChannel { channel }) => {
					T::Hrmp::establish_system_channel(channel)?
				},
			}
			Ok(())
		}

		/// Serve a parachain's own HRMP request, as that parachain.
		///
		/// The asking para is the one the origin resolves to.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::relay_request())]
		pub fn relay_request(origin: OriginFor<T>, request: ParaRequest) -> DispatchResult {
			let para: ParaId = T::ParachainOrigin::ensure_origin(origin)?.into();
			ensure!(T::AdmitRequest::admit(para), Error::<T>::RequestRefused);

			let ParaRequest::V1(request) = request;
			match request {
				ParaRequestV1::InitOpenChannel {
					recipient,
					proposed_max_capacity,
					proposed_max_message_size,
				} => T::Hrmp::init_open_channel(
					para,
					recipient,
					proposed_max_capacity,
					proposed_max_message_size,
				),
				ParaRequestV1::AcceptOpenChannel { sender } => {
					T::Hrmp::accept_open_channel(para, sender)
				},
				ParaRequestV1::CloseChannel { channel } => T::Hrmp::close_channel(para, channel),
				ParaRequestV1::CancelOpenRequest { channel, open_requests } => {
					T::Hrmp::cancel_open_request(para, channel, open_requests)
				},
				ParaRequestV1::EstablishChannelWithSystem { target_system_chain } => {
					T::Hrmp::establish_channel_with_system(para, target_system_chain)
				},
			}?;

			Self::deposit_event(Event::RequestServed { para });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Ask the parachain to hold `amount` for `key`. Queued releases are sent first.
		///
		/// `Err(())` means the hold could not be handed to the transport.
		#[allow(clippy::result_unit_err)]
		pub fn hold(key: DepositKey, amount: Balance) -> Result<(), ()> {
			Self::flush_releases();
			T::SendToPara::send(MessageToPara::V1(MessageToParaV1::Hold { key, amount }))?;
			Self::deposit_event(Event::HoldSent { key, amount });
			Ok(())
		}

		/// Queue a release of `amount` of what is held for `key`, or all of it if `None`.
		pub fn release(key: DepositKey, amount: Option<Balance>) {
			PendingReleases::<T>::append((key, amount));
		}

		/// Send every queued release, oldest first. Returns how many were queued.
		pub(crate) fn flush_releases() -> u32 {
			let pending = PendingReleases::<T>::take();
			for (key, amount) in pending.iter().copied() {
				let sent = T::SendToPara::send(MessageToPara::V1(MessageToParaV1::Release {
					key,
					amount,
				}));
				if sent.is_ok() {
					Self::deposit_event(Event::ReleaseSent { key, amount });
				} else {
					log::error!(
						target: "runtime::hrmp-relay",
						"failed to send the release of {key:?}",
					);
					Self::deposit_event(Event::ReleaseFailed { key, amount });
				}
			}
			pending.len() as u32
		}
	}
}
