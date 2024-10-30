// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implementation for [`snowbridge_core::outbound::SendMessage`]
use super::*;
use bridge_hub_common::AggregateMessageOrigin;
use codec::Encode;
use frame_support::{
	ensure,
	traits::{EnqueueMessage, Get},
	CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use frame_system::unique;
use snowbridge_core::{
	outbound::{
		v1::{Fee, Message, QueuedMessage, SendError, SendMessage, VersionedQueuedMessage},
		SendMessageFeeProvider,
	},
	ChannelId, PRIMARY_GOVERNANCE_CHANNEL,
};
use sp_core::H256;
use sp_runtime::BoundedVec;

/// The maximal length of an enqueued message, as determined by the MessageQueue pallet
pub type MaxEnqueuedMessageSizeOf<T> =
	<<T as Config>::MessageQueue as EnqueueMessage<AggregateMessageOrigin>>::MaxMessageLen;

#[derive(Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound)]
pub struct Ticket<T>
where
	T: Config,
{
	pub message_id: H256,
	pub channel_id: ChannelId,
	pub message: BoundedVec<u8, MaxEnqueuedMessageSizeOf<T>>,
}

impl<T> SendMessage for Pallet<T>
where
	T: Config,
{
	type Ticket = Ticket<T>;

	fn validate(
		message: &Message,
	) -> Result<(Self::Ticket, Fee<<Self as SendMessageFeeProvider>::Balance>), SendError> {
		// The inner payload should not be too large
		let payload = message.command.abi_encode();
		ensure!(
			payload.len() < T::MaxMessagePayloadSize::get() as usize,
			SendError::MessageTooLarge
		);

		// Ensure there is a registered channel we can transmit this message on
		ensure!(T::Channels::contains(&message.channel_id), SendError::InvalidChannel);

		// Generate a unique message id unless one is provided
		let message_id: H256 = message
			.id
			.unwrap_or_else(|| unique((message.channel_id, &message.command)).into());

		let gas_used_at_most = T::GasMeter::maximum_gas_used_at_most(&message.command);
		let fee = Self::calculate_fee(gas_used_at_most, T::PricingParameters::get());

		let queued_message: VersionedQueuedMessage = QueuedMessage {
			id: message_id,
			channel_id: message.channel_id,
			command: message.command.clone(),
		}
		.into();
		// The whole message should not be too large
		let encoded = queued_message.encode().try_into().map_err(|_| SendError::MessageTooLarge)?;

		let ticket = Ticket { message_id, channel_id: message.channel_id, message: encoded };

		Ok((ticket, fee))
	}

	fn deliver(ticket: Self::Ticket) -> Result<H256, SendError> {
		let origin = AggregateMessageOrigin::Snowbridge(ticket.channel_id);

		if ticket.channel_id != PRIMARY_GOVERNANCE_CHANNEL {
			ensure!(!Self::operating_mode().is_halted(), SendError::Halted);
		}

		let message = ticket.message.as_bounded_slice();

		T::MessageQueue::enqueue_message(message, origin);
		Self::deposit_event(Event::MessageQueued { id: ticket.message_id });
		Ok(ticket.message_id)
	}
}

impl<T: Config> SendMessageFeeProvider for Pallet<T> {
	type Balance = T::Balance;

	/// The local component of the message processing fees in native currency
	fn local_fee() -> Self::Balance {
		Self::calculate_local_fee()
	}
}
