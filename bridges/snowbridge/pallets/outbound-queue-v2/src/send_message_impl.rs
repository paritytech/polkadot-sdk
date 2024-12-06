// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implementation for [`snowbridge_outbound_primitives::outbound::v2::SendMessage`]
use super::*;
use bridge_hub_common::AggregateMessageOrigin;
use codec::Encode;
use frame_support::{
	ensure,
	traits::{EnqueueMessage, Get},
};
use snowbridge_outbound_primitives::{
	v2::{primary_governance_origin, Message, SendMessage},
	SendError, SendMessageFeeProvider,
};
use sp_core::H256;
use sp_runtime::BoundedVec;

impl<T> SendMessage for Pallet<T>
where
	T: Config,
{
	type Ticket = Message;

	type Balance = T::Balance;

	fn validate(message: &Message) -> Result<(Self::Ticket, Self::Balance), SendError> {
		// The inner payload should not be too large
		let payload = message.encode();
		ensure!(
			payload.len() < T::MaxMessagePayloadSize::get() as usize,
			SendError::MessageTooLarge
		);

		let fee = Self::calculate_local_fee();

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
