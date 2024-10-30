// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implementation for [`snowbridge_core::outbound::SendMessage`]
use super::*;
use bridge_hub_common::AggregateMessageOrigin;
use codec::Encode;
use frame_support::{
	ensure,
	traits::{EnqueueMessage, Get},
};
use snowbridge_core::outbound::{
	v2::{primary_governance_origin, Message, SendError, SendMessage},
	SendMessageFeeProvider,
};
use sp_core::H256;
use sp_runtime::BoundedVec;

/// The maximal length of an enqueued message, as determined by the MessageQueue pallet
pub type MaxEnqueuedMessageSizeOf<T> =
	<<T as Config>::MessageQueue as EnqueueMessage<AggregateMessageOrigin>>::MaxMessageLen;

impl<T> SendMessage for Pallet<T>
where
	T: Config,
{
	type Ticket = Message;

	fn validate(
		message: &Message,
	) -> Result<(Self::Ticket, Fee<<Self as SendMessageFeeProvider>::Balance>), SendError> {
		// The inner payload should not be too large
		let payload = message.encode();
		ensure!(
			payload.len() < T::MaxMessagePayloadSize::get() as usize,
			SendError::MessageTooLarge
		);

		let fee = Fee::from(Self::calculate_local_fee());

		Ok((message.clone(), fee))
	}

	fn deliver(ticket: Self::Ticket) -> Result<H256, SendError> {
		let origin = AggregateMessageOrigin::SnowbridgeV2(ticket.origin);

		if ticket.origin != primary_governance_origin() {
			ensure!(!Self::operating_mode().is_halted(), SendError::Halted);
		}

		let message =
			BoundedVec::try_from(ticket.encode()).map_err(|_| SendError::MessageTooLarge)?;

		T::MessageQueue::enqueue_message(message.as_bounded_slice(), origin);
		Self::deposit_event(Event::MessageQueued { message: ticket.clone() });
		Ok(ticket.id)
	}
}

impl<T: Config> SendMessageFeeProvider for Pallet<T> {
	type Balance = T::Balance;

	/// The local component of the message processing fees in native currency
	fn local_fee() -> Self::Balance {
		Self::calculate_local_fee()
	}
}
