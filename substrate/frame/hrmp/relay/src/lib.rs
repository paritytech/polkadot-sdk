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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::EnsureOrigin;
use hrmp_primitives::{
	ChannelId, FailureReason, HrmpRegistry, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, ParaId,
};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

// TODO: `benchmarking.rs`, one benchmark per handler, once the bodies land.
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

		/// The relay chain's HRMP channel registry.
		type Registry: HrmpRegistry;

		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// Every emitter is still a `todo!()`.
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
		/// A report could not be sent back to the parachain.
		ReportFailed {
			/// The para the report was about.
			para_id: ParaId,
			/// The id of the message the report concludes.
			message_id: u64,
		},
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
			}

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Hand a report to the transport.
		///
		/// A transport failure is only logged and surfaced as an event: every caller has already
		/// committed relay-chain state that must not be unwound just because the report bounced.
		#[allow(dead_code)]
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
			let _ = (channel, message_id, max_capacity, max_message_size);
			todo!()
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
	}
}
