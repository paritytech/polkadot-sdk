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

//! # Parachain HRMP pallet
//!
//! User-facing half of HRMP channel management. Runs on a parachain, holding channel deposits and
//! driving open / accept / close on the relay-chain counterpart (`pallet-hrmp-relay`) over XCM.
//!
//! Channel state and message routing stay on the relay chain. What lives here is the intent and
//! the money: a request is recorded and its deposits held before the relay chain is asked, and
//! the deposits are only settled by the relay chain's answer, which arrives through
//! [`Call::receive`].
//!
//! A para with no channel to this chain cannot reach it directly. Those paras ask the relay
//! chain, which authenticates them and forwards the request to [`Call::receive_request`].

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::{Consideration, Contains, EnsureOrigin, Footprint};
use hrmp_primitives::{
	ChannelId, FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, Outcome, ParaId,
	ParaRequest, ParaRequestV1,
};
use scale_info::TypeInfo;
use sp_runtime::{traits::Convert, DispatchResult};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

// TODO: `benchmarking.rs`, one benchmark per extrinsic, once the bodies land.
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Used to send an XCM `Transact` to the HRMP pallet on the remote relay chain.
pub trait SendToRelay {
	/// Send `message` to the relay chain.
	///
	/// `Err(())` means the message could not be handed to the transport at all. Callers are
	/// expected to fail the whole extrinsic, so nothing is left half-done.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToRelay) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToRelay for () {
	fn send(_message: MessageToRelay) -> Result<(), ()> {
		Ok(())
	}
}

/// A request the relay chain has not answered yet.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum RequestState<SenderTicket, RecipientTicket> {
	/// The sender asked, the recipient has not accepted.
	Requested {
		/// The sender's held deposit.
		sender_deposit: SenderTicket,
	},
	/// The relay chain has been asked to open the channel.
	Accepted {
		/// The sender's held deposit.
		sender_deposit: SenderTicket,
		/// The recipient's held deposit.
		recipient_deposit: RecipientTicket,
		/// Which call asked, so the answer can be reported under the right event.
		kind: OpenKind,
	},
}

/// Which call put a request into [`RequestState::Accepted`].
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum OpenKind {
	/// `hrmp_accept_open_channel`.
	Agreed,
	/// `force_open_hrmp_channel`.
	Forced,
	/// `establish_system_channel`.
	System,
	/// `establish_channel_with_system`.
	SystemPair,
}

/// A close the parachain has asked the relay chain to enact.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct CloseRequest {
	/// Which end asked.
	pub initiator: ParaId,
	/// The id of the message that asked the relay chain.
	pub message_id: u64,
}

/// A pending open request, with the sizes it asked for.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ChannelRequest<SenderTicket, RecipientTicket> {
	/// How far the request has got.
	pub state: RequestState<SenderTicket, RecipientTicket>,
	/// How many messages the channel may hold at once.
	pub max_capacity: u32,
	/// The largest message the channel will carry.
	pub max_message_size: u32,
	/// The id of the message that asked the relay chain, echoed in its answer.
	pub message_id: u64,
}

/// A channel the relay chain has confirmed is open.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ChannelInfo<SenderTicket, RecipientTicket> {
	/// How many messages the channel may hold at once.
	pub max_capacity: u32,
	/// The largest message the channel will carry.
	pub max_message_size: u32,
	/// The sender's held deposit.
	pub sender_deposit: SenderTicket,
	/// The recipient's held deposit.
	pub recipient_deposit: RecipientTicket,
}

/// [`ChannelRequest`] as this pallet stores it.
pub type ChannelRequestOf<T> =
	ChannelRequest<<T as Config>::SenderConsideration, <T as Config>::RecipientConsideration>;

/// [`ChannelInfo`] as this pallet stores it.
pub type ChannelInfoOf<T> =
	ChannelInfo<<T as Config>::SenderConsideration, <T as Config>::RecipientConsideration>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The cost the sender pays to open a channel.
		///
		/// Footprint is a single item sized as the channel's capacity, so either a flat or a
		/// per-message price fits. A system chain on either end pays nothing.
		type SenderConsideration: Consideration<Self::AccountId, Footprint>;

		/// The cost the recipient pays to accept a channel.
		type RecipientConsideration: Consideration<Self::AccountId, Footprint>;

		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay;

		/// An origin that is sure to be the relay chain's HRMP pallet.
		type RelayOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The origin that can perform "force" actions on channels.
		type ChannelManager: EnsureOrigin<Self::RuntimeOrigin>;

		/// The account a para's deposits are taken from, its sovereign account on this chain.
		type SovereignAccountOf: Convert<ParaId, Self::AccountId>;

		/// Mirror of the relay chain's `hrmp_channel_max_capacity`.
		#[pallet::constant]
		type MaxCapacity: Get<u32>;

		/// Mirror of the relay chain's `hrmp_channel_max_message_size`.
		#[pallet::constant]
		type MaxMessageSize: Get<u32>;

		/// Mirror of the relay chain's `hrmp_max_parachain_inbound_channels`.
		#[pallet::constant]
		type MaxInboundChannels: Get<u32>;

		/// Mirror of the relay chain's `hrmp_max_parachain_outbound_channels`.
		#[pallet::constant]
		type MaxOutboundChannels: Get<u32>;

		/// The `(max_message_size, max_capacity)` used for channels involving a system chain.
		///
		/// Size first, as on the relay chain. Everything on the wire goes capacity first.
		type DefaultChannelSizeAndCapacityWithSystem: Get<(u32, u32)>;

		/// The paras a channel needs no deposit for, and the only ones a system channel may join.
		type IsSystemPara: Contains<ParaId>;

		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Hold reasons for runtimes that pay the deposits out of held funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The deposit the sender puts up to open a channel.
		#[codec(index = 0)]
		SenderDeposit,
		/// The deposit the recipient puts up to accept a channel.
		#[codec(index = 1)]
		RecipientDeposit,
	}

	/// Open requests the relay chain has not confirmed yet.
	#[pallet::storage]
	pub type Requests<T: Config> = StorageMap<_, Blake2_128Concat, ChannelId, ChannelRequestOf<T>>;

	/// Channels the relay chain has confirmed are open.
	#[pallet::storage]
	pub type Channels<T: Config> = StorageMap<_, Blake2_128Concat, ChannelId, ChannelInfoOf<T>>;

	/// Senders that have a channel to a recipient, sorted.
	/// Closes the relay chain has been asked to enact, by channel.
	#[pallet::storage]
	pub type CloseRequests<T: Config> = StorageMap<_, Blake2_128Concat, ChannelId, CloseRequest>;

	#[pallet::storage]
	pub type IngressIndex<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ParaId,
		BoundedVec<ParaId, <T as Config>::MaxInboundChannels>,
		ValueQuery,
	>;

	/// Recipients a sender has a channel to, sorted.
	#[pallet::storage]
	pub type EgressIndex<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ParaId,
		BoundedVec<ParaId, <T as Config>::MaxOutboundChannels>,
		ValueQuery,
	>;

	/// How many open requests a para has initiated.
	#[pallet::storage]
	pub type OpenRequestCount<T: Config> = StorageMap<_, Twox64Concat, ParaId, u32, ValueQuery>;

	/// How many open requests a para has accepted.
	#[pallet::storage]
	pub type AcceptedRequestCount<T: Config> = StorageMap<_, Twox64Concat, ParaId, u32, ValueQuery>;

	/// The id the next message to the relay chain will carry.
	#[pallet::storage]
	pub type NextMessageId<T: Config> = StorageValue<_, u64, ValueQuery>;

	// Every emitter is still a `todo!()`.
	#[allow(dead_code)]
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A sender asked to open a channel.
		OpenChannelRequested {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
			/// How many messages the channel may hold at once.
			proposed_max_capacity: u32,
			/// The largest message the channel will carry.
			proposed_max_message_size: u32,
		},
		/// A recipient accepted an open request.
		OpenChannelAccepted {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
		},
		/// The relay chain confirmed a channel is open.
		ChannelOpened {
			/// The channel.
			channel: ChannelId,
			/// The id of the message this concludes.
			message_id: u64,
		},
		/// The relay chain refused to open a channel. Deposits are released.
		OpenChannelFailed {
			/// The channel.
			channel: ChannelId,
			/// The id of the message this concludes.
			message_id: u64,
			/// Why the relay chain refused.
			reason: FailureReason,
		},
		/// An open request was withdrawn before the recipient accepted it.
		OpenChannelCanceled {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
			/// Which end withdrew it.
			by_parachain: ParaId,
		},
		/// One end asked to close a channel.
		ChannelClosedPending {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
			/// Which end asked.
			by_parachain: ParaId,
		},
		/// The relay chain confirmed a channel is closed. Deposits are released.
		ChannelCloseDone {
			/// The channel.
			channel: ChannelId,
			/// The id of the message this concludes.
			message_id: u64,
		},
		/// A deposit-free channel with a system chain was asked for.
		SystemChannelRequested {
			/// The channel. Both directions are opened.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
		},
		/// The relay chain confirmed a deposit-free channel with a system chain is open.
		HrmpSystemChannelOpened {
			/// The channel.
			channel: ChannelId,
			/// How many messages the channel may hold at once.
			proposed_max_capacity: u32,
			/// The largest message the channel will carry.
			proposed_max_message_size: u32,
		},
		/// A channel was opened without the recipient's consent.
		ForceOpenRequested {
			/// The channel.
			channel: ChannelId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
		},
		/// The relay chain confirmed a force-opened channel is open.
		HrmpChannelForceOpened {
			/// The channel.
			channel: ChannelId,
			/// How many messages the channel may hold at once.
			proposed_max_capacity: u32,
			/// The largest message the channel will carry.
			proposed_max_message_size: u32,
		},
		/// Every channel and request of a para was dropped.
		ForceCleanExecuted {
			/// The para.
			para_id: ParaId,
			/// The id of the message that asked the relay chain.
			message_id: u64,
		},
		/// Confirmed requests were opened ahead of the relay chain's session boundary.
		ForceProcessedOpen {
			/// How many requests were processed.
			channels: u32,
		},
		/// Close requests were enacted ahead of the relay chain's session boundary.
		ForceProcessedClose {
			/// How many channels were closed.
			channels: u32,
		},
		/// A channel's deposits were brought in line with the current prices.
		OpenChannelDepositsUpdated {
			/// The channel.
			channel: ChannelId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A para asked to open a channel to itself.
		OpenHrmpChannelToSelf,
		/// The recipient is not a para this chain will open a channel to.
		OpenHrmpChannelInvalidRecipient,
		/// The proposed capacity is zero.
		OpenHrmpChannelZeroCapacity,
		/// The proposed capacity is above `MaxCapacity`.
		OpenHrmpChannelCapacityExceedsLimit,
		/// The proposed message size is zero.
		OpenHrmpChannelZeroMessageSize,
		/// The proposed message size is above `MaxMessageSize`.
		OpenHrmpChannelMessageSizeExceedsLimit,
		/// The channel is already open.
		OpenHrmpChannelAlreadyExists,
		/// A request for this channel is already recorded.
		OpenHrmpChannelAlreadyRequested,
		/// The sender has as many outbound channels as it is allowed.
		OpenHrmpChannelLimitExceeded,
		/// There is no request for this channel to accept.
		AcceptHrmpChannelDoesntExist,
		/// The request has already been accepted.
		AcceptHrmpChannelAlreadyConfirmed,
		/// The recipient has as many inbound channels as it is allowed.
		AcceptHrmpChannelLimitExceeded,
		/// The caller is neither end of the channel it asked to close.
		CloseHrmpChannelUnauthorized,
		/// There is no such channel to close.
		CloseHrmpChannelDoesntExist,
		/// A close for this channel is already underway.
		CloseHrmpChannelAlreadyUnderway,
		/// The caller is neither end of the request it asked to cancel.
		CancelHrmpOpenChannelUnauthorized,
		/// There is no request for this channel.
		OpenHrmpChannelDoesntExist,
		/// The request has already been accepted, so it cannot be cancelled.
		OpenHrmpChannelAlreadyConfirmed,
		/// The witness count the caller passed does not match storage.
		WrongWitness,
		/// Neither end of the channel is a system chain.
		ChannelCreationNotAuthorized,
		/// The relay chain's answer does not match anything this chain is waiting for.
		UnexpectedResponse,
		/// The message could not be handed to the transport.
		SendFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Accept a report from the relay chain's HRMP pallet.
		///
		/// Not callable by users: the origin must be the relay chain.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::receive())]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			T::RelayOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToPara::V1(MessageToParaV1::OpenChannelResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_open_channel_response(channel, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::CloseResponse {
					channel,
					message_id,
					outcome,
				}) => Self::on_close_response(channel, message_id, outcome),
			}
		}

		#[pallet::call_index(1)]
		#[pallet::weight(match request {
			ParaRequest::V1(ParaRequestV1::InitOpenChannel { .. }) =>
				T::WeightInfo::hrmp_init_open_channel(),
			ParaRequest::V1(ParaRequestV1::AcceptOpenChannel { .. }) =>
				T::WeightInfo::hrmp_accept_open_channel(),
			ParaRequest::V1(ParaRequestV1::CloseChannel { .. }) =>
				T::WeightInfo::hrmp_close_channel(),
			ParaRequest::V1(ParaRequestV1::CancelOpenRequest { open_requests, .. }) =>
				T::WeightInfo::hrmp_cancel_open_request(*open_requests),
			ParaRequest::V1(ParaRequestV1::EstablishChannelWithSystem { .. }) =>
				T::WeightInfo::establish_channel_with_system(),
		})]
		pub fn receive_request(
			origin: OriginFor<T>,
			para_id: ParaId,
			request: ParaRequest,
		) -> DispatchResult {
			T::RelayOrigin::ensure_origin(origin)?;

			match request {
				ParaRequest::V1(ParaRequestV1::InitOpenChannel {
					recipient,
					proposed_max_capacity,
					proposed_max_message_size,
				}) => Self::on_init_open_channel(
					para_id,
					recipient,
					proposed_max_capacity,
					proposed_max_message_size,
				),
				ParaRequest::V1(ParaRequestV1::AcceptOpenChannel { sender }) => {
					Self::on_accept_open_channel(para_id, sender)
				},
				ParaRequest::V1(ParaRequestV1::CloseChannel { channel }) => {
					Self::on_close_channel(para_id, channel)
				},
				ParaRequest::V1(ParaRequestV1::CancelOpenRequest { channel, open_requests }) => {
					Self::on_cancel_open_request(para_id, channel, open_requests)
				},
				ParaRequest::V1(ParaRequestV1::EstablishChannelWithSystem {
					target_system_chain,
				}) => Self::on_establish_channel_with_system(para_id, target_system_chain),
			}
		}

		/// Drop every channel and request belonging to `para`.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::force_clean_hrmp(*num_inbound, *num_outbound))]
		pub fn force_clean_hrmp(
			origin: OriginFor<T>,
			para: ParaId,
			num_inbound: u32,
			num_outbound: u32,
		) -> DispatchResult {
			let _ = (origin, para, num_inbound, num_outbound);
			todo!()
		}

		/// Open every confirmed request now, rather than at the next session boundary.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::force_process_hrmp_open(*channels))]
		pub fn force_process_hrmp_open(origin: OriginFor<T>, channels: u32) -> DispatchResult {
			let _ = (origin, channels);
			todo!()
		}

		/// Enact every close request now, rather than at the next session boundary.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::force_process_hrmp_close(*channels))]
		pub fn force_process_hrmp_close(origin: OriginFor<T>, channels: u32) -> DispatchResult {
			let _ = (origin, channels);
			todo!()
		}

		/// Open a channel without the recipient's consent.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::force_open_hrmp_channel(1))]
		pub fn force_open_hrmp_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
			max_capacity: u32,
			max_message_size: u32,
		) -> DispatchResultWithPostInfo {
			let _ = (origin, sender, recipient, max_capacity, max_message_size);
			todo!()
		}

		/// Bring a channel's deposits in line with the current prices.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::poke_channel_deposits())]
		pub fn poke_channel_deposits(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			let _ = (origin, sender, recipient);
			todo!()
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::establish_system_channel())]
		pub fn establish_system_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			let _ = (origin, sender, recipient);
			todo!()
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The footprint one side of a channel with this capacity is priced by.
	pub fn channel_footprint(max_capacity: u32) -> Footprint {
		Footprint::from_parts(1, max_capacity as usize)
	}

	#[allow(dead_code)]
	fn next_message_id() -> u64 {
		NextMessageId::<T>::mutate(|next| {
			let id = *next;
			*next = next.wrapping_add(1);
			id
		})
	}

	/// `hrmp_init_open_channel`, asked for by `sender` through the relay chain.
	fn on_init_open_channel(
		sender: ParaId,
		recipient: ParaId,
		proposed_max_capacity: u32,
		proposed_max_message_size: u32,
	) -> DispatchResult {
		let _ = (sender, recipient, proposed_max_capacity, proposed_max_message_size);
		todo!()
	}

	/// `hrmp_accept_open_channel`, asked for by `recipient` through the relay chain.
	fn on_accept_open_channel(recipient: ParaId, sender: ParaId) -> DispatchResult {
		let _ = (recipient, sender);
		todo!()
	}

	/// `hrmp_close_channel`, asked for by `initiator` through the relay chain.
	fn on_close_channel(initiator: ParaId, channel: ChannelId) -> DispatchResult {
		let _ = (initiator, channel);
		todo!()
	}

	/// `hrmp_cancel_open_request`, asked for by `initiator` through the relay chain.
	fn on_cancel_open_request(
		initiator: ParaId,
		channel: ChannelId,
		open_requests: u32,
	) -> DispatchResult {
		let _ = (initiator, channel, open_requests);
		todo!()
	}

	/// `establish_channel_with_system`, asked for by `sender` through the relay chain.
	fn on_establish_channel_with_system(
		sender: ParaId,
		target_system_chain: ParaId,
	) -> DispatchResult {
		let _ = (sender, target_system_chain);
		todo!()
	}

	fn on_open_channel_response(
		channel: ChannelId,
		message_id: u64,
		outcome: Result<(u32, u32), FailureReason>,
	) -> DispatchResult {
		let _ = (channel, message_id, outcome);
		todo!()
	}

	fn on_close_response(channel: ChannelId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let _ = (channel, message_id, outcome);
		todo!()
	}
}
