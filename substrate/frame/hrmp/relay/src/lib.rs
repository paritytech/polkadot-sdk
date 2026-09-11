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

//! # Relay-chain HRMP pallet
//!
//! Relay half of HRMP channel management. Runs on the relay chain, applying channel operations
//! received from a parachain (`pallet-hrmp-para`) to the relay's `hrmp` routing table through
//! [`hrmp_primitives::HrmpRegistry`], and reporting the outcome back.
//!
//! Holds no state of its own. Every operation is deposit-free here: the parachain holds the money.
//!
//! It also relays both other directions: [`Call::relay_request`] takes a request from any
//! parachain and forwards it to the control-plane parachain with the asking para's id attached,
//! and [`MessageToRelayV1::NotifyPara`] carries what the control-plane parachain has to tell a
//! para out to it. The relay chain reaches every para; the control-plane parachain does not.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{storage::with_storage_layer, traits::EnsureOrigin};
use hrmp_primitives::{
	ChannelId, FailureReason, HrmpRegistry, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, ParaId, ParaNotification, ParaRequest,
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

/// Used to send an XCM `Transact` to the HRMP pallet on the remote parachain.
pub trait SendToPara {
	/// Send `message` to the parachain.
	///
	/// `Err(())` means the transport refused the message. Callers here have already committed
	/// relay-chain state that must survive, so they log and carry on rather than unwinding.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToPara) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
}

/// Used to send an XCM `Transact` forwarding a parachain's request to the HRMP pallet on the
/// remote parachain.
pub trait ForwardToPara {
	/// Forward `request`, asked for by `para_id`, to the parachain.
	///
	/// `Err(())` means the message could not be handed to the transport at all. Nothing has been
	/// committed by then, so the caller fails the whole extrinsic.
	#[allow(clippy::result_unit_err)]
	fn forward(para_id: ParaId, request: ParaRequest) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl ForwardToPara for () {
	fn forward(_para_id: ParaId, _request: ParaRequest) -> Result<(), ()> {
		Ok(())
	}
}

/// Used to deliver a channel notification to a parachain, as the matching XCM instruction.
pub trait NotifyParachain {
	/// Tell `para_id` about a channel it is one end of.
	///
	/// `Err(())` means the message could not be handed to the transport at all. The caller has
	/// already committed the state the notification is about, so it logs rather than unwinding.
	#[allow(clippy::result_unit_err)]
	fn notify(para_id: ParaId, notification: ParaNotification) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl NotifyParachain for () {
	fn notify(_para_id: ParaId, _notification: ParaNotification) -> Result<(), ()> {
		Ok(())
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

		/// A trusted parachain authorized to drive HRMP channel management.
		type ParaOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Sends messages to the parachain.
		type SendToPara: SendToPara;

		/// An origin any parachain uses to act as itself, resolved to its para id.
		type ParachainOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = ParaId>;

		/// Forwards parachain requests to the parachain that owns channel management.
		type ForwardToPara: ForwardToPara;

		/// Delivers channel notifications to any parachain.
		type NotifyParachain: NotifyParachain;

		/// The relay chain's HRMP channel registry.
		type Registry: HrmpRegistry;

		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// Every emitter but `RequestForwarded` is still a `todo!()`.
	#[allow(dead_code)]
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A channel was opened.
		ChannelOpened {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked for it.
			message_id: u64,
		},
		/// A request to open a channel was refused.
		OpenChannelRejected {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked for it.
			message_id: u64,
			/// Why it was refused.
			reason: FailureReason,
		},
		/// A channel was closed.
		ChannelClosed {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked for it.
			message_id: u64,
		},
		/// Every channel of a para was dropped.
		ChannelsCleaned {
			/// The para.
			para_id: ParaId,
			/// The id of the message that asked for it.
			message_id: u64,
		},
		/// A channel notification could not be delivered to a para.
		NotifyFailed {
			/// The para that was to be told.
			para_id: ParaId,
		},
		/// A parachain's request was forwarded to the parachain that owns channel management.
		RequestForwarded {
			/// The para that asked.
			para_id: ParaId,
		},
		/// A report could not be sent back to the parachain.
		ReportFailed {
			/// The para the report was about.
			para_id: ParaId,
			/// The id of the message the report concludes.
			message_id: u64,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The request could not be handed to the transport.
		ForwardFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Accept a request from the parachain's HRMP pallet.
		///
		/// Not callable by users: the origin must be the parachain that owns channel management.
		#[pallet::call_index(0)]
		#[pallet::weight(match message {
			MessageToRelay::V1(MessageToRelayV1::OpenChannel { .. }) =>
				T::WeightInfo::receive_open_channel(),
			MessageToRelay::V1(MessageToRelayV1::ForceOpenChannel { .. }) =>
				T::WeightInfo::receive_force_open_channel(),
			MessageToRelay::V1(MessageToRelayV1::OpenSystemChannel { .. }) =>
				T::WeightInfo::receive_open_system_channel(),
			MessageToRelay::V1(MessageToRelayV1::OpenSystemPair { .. }) =>
				T::WeightInfo::receive_open_system_pair(),
			MessageToRelay::V1(MessageToRelayV1::CloseChannel { .. }) =>
				T::WeightInfo::receive_close_channel(),
			MessageToRelay::V1(MessageToRelayV1::ForceClean { .. }) =>
				T::WeightInfo::receive_force_clean(),
			MessageToRelay::V1(MessageToRelayV1::NotifyPara { .. }) =>
				T::WeightInfo::receive_notify_para(),
		})]
		pub fn receive(origin: OriginFor<T>, message: MessageToRelay) -> DispatchResult {
			T::ParaOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToRelay::V1(MessageToRelayV1::OpenChannel {
					channel,
					message_id,
					max_capacity,
					max_message_size,
				}) => Self::on_open_channel(channel, message_id, max_capacity, max_message_size),
				MessageToRelay::V1(MessageToRelayV1::ForceOpenChannel {
					channel,
					message_id,
					max_capacity,
					max_message_size,
				}) => {
					Self::on_force_open_channel(channel, message_id, max_capacity, max_message_size)
				},
				MessageToRelay::V1(MessageToRelayV1::OpenSystemChannel { channel, message_id }) => {
					Self::on_open_system_channel(channel, message_id)
				},
				MessageToRelay::V1(MessageToRelayV1::OpenSystemPair {
					channel,
					message_id,
					max_capacity,
					max_message_size,
				}) => Self::on_open_system_pair(channel, message_id, max_capacity, max_message_size),
				MessageToRelay::V1(MessageToRelayV1::CloseChannel {
					channel,
					message_id,
					initiator,
				}) => Self::on_close_channel(channel, message_id, initiator),
				MessageToRelay::V1(MessageToRelayV1::ForceClean { para_id, message_id }) => {
					Self::on_force_clean(para_id, message_id)
				},
				MessageToRelay::V1(MessageToRelayV1::NotifyPara { para_id, notification }) => {
					Self::on_notify_para(para_id, notification)
				},
			}

			Ok(())
		}

		/// Forward a parachain's own channel request to the parachain that owns channel
		/// management.
		///
		/// Callable by any parachain. The asking para is the one the origin resolves to, not
		/// anything in the payload, so a para can only ask on its own behalf. Nothing is checked
		/// here beyond the origin: the request is validated where the channel state and the
		/// deposits live.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::relay_request())]
		pub fn relay_request(origin: OriginFor<T>, request: ParaRequest) -> DispatchResult {
			let para_id = T::ParachainOrigin::ensure_origin(origin)?;

			T::ForwardToPara::forward(para_id, request).map_err(|()| {
				log::error!(
					target: "runtime::hrmp-relay",
					"failed to forward the request from para {para_id} to the parachain",
				);
				Error::<T>::ForwardFailed
			})?;

			Self::deposit_event(Event::RequestForwarded { para_id });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Hand a report to the transport.
		///
		/// A transport failure is only logged and surfaced as an event: every caller has already
		/// committed relay-chain state that must not be unwound just because the report bounced.
		fn report(para_id: ParaId, message_id: u64, message: MessageToParaV1) {
			if T::SendToPara::send(MessageToPara::V1(message)).is_err() {
				log::error!(
					target: "runtime::hrmp-relay",
					"failed to report the outcome for para {para_id} back to the parachain",
				);
				Self::deposit_event(Event::ReportFailed { para_id, message_id });
			}
		}

		fn on_open_channel(
			channel: ChannelId,
			message_id: u64,
			max_capacity: u32,
			max_message_size: u32,
		) {
			// A refusal is reported rather than raised, so this call always succeeds and the
			// dispatch's own layer never unwinds. The registry is not required to be atomic on
			// failure, so it gets one of its own.
			let outcome = with_storage_layer(|| {
				T::Registry::open_channel(channel, max_capacity, max_message_size)
					.map(|()| (max_capacity, max_message_size))
			});

			let notification = match &outcome {
				Ok(_) => {
					Self::deposit_event(Event::ChannelOpened { channel, message_id });
					ParaNotification::ChannelOpened { channel }
				},
				Err(reason) => {
					Self::deposit_event(Event::OpenChannelRejected {
						channel,
						message_id,
						reason: reason.clone(),
					});
					ParaNotification::ChannelOpenFailure { channel, reason: reason.clone() }
				},
			};

			// Both ends asked for this channel, and only the relay chain knows whether it exists.
			Self::on_notify_para(channel.sender, notification.clone());
			Self::on_notify_para(channel.recipient, notification);

			Self::report(
				channel.sender,
				message_id,
				MessageToParaV1::OpenChannelResponse { channel, message_id, outcome },
			);
		}

		fn on_force_open_channel(
			channel: ChannelId,
			message_id: u64,
			max_capacity: u32,
			max_message_size: u32,
		) {
			let _ = (channel, message_id, max_capacity, max_message_size);
			todo!()
		}

		fn on_open_system_channel(channel: ChannelId, message_id: u64) {
			let _ = (channel, message_id);
			todo!()
		}

		fn on_open_system_pair(
			channel: ChannelId,
			message_id: u64,
			max_capacity: u32,
			max_message_size: u32,
		) {
			let _ = (channel, message_id, max_capacity, max_message_size);
			todo!()
		}

		fn on_close_channel(channel: ChannelId, message_id: u64, initiator: ParaId) {
			let _ = (channel, message_id, initiator);
			todo!()
		}

		fn on_force_clean(para_id: ParaId, message_id: u64) {
			let _ = (para_id, message_id);
			todo!()
		}

		/// Hand a notification to the transport that reaches any para.
		///
		/// A transport failure is only logged and surfaced as an event: the state the para is
		/// being told about is already committed on both chains.
		fn on_notify_para(para_id: ParaId, notification: ParaNotification) {
			if T::NotifyParachain::notify(para_id, notification).is_err() {
				log::error!(
					target: "runtime::hrmp-relay",
					"failed to deliver the channel notification for para {para_id}",
				);
				Self::deposit_event(Event::NotifyFailed { para_id });
			}
		}
	}
}
