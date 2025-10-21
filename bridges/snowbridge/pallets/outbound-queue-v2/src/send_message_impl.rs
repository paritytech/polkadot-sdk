// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Implementation for [`snowbridge_outbound_queue_primitives::v2::SendMessage`]
use super::*;
use bridge_hub_common::AggregateMessageOrigin;
use codec::Encode;
use frame_support::{
	ensure,
	traits::{EnqueueMessage, Get},
};
use snowbridge_core::AgentIdOf;
use snowbridge_outbound_queue_primitives::{
	v2::{Message, SendMessage},
	SendError,
};
use sp_core::H256;
use sp_runtime::BoundedVec;
use xcm::{latest::ParentThen, prelude::Parachain};
use xcm_executor::traits::ConvertLocation;

impl<T> SendMessage for Pallet<T>
where
	T: Config,
{
	type Ticket = Message;

	fn validate(message: &Message) -> Result<Self::Ticket, SendError> {
		// The inner payload should not be too large
		let payload = message.encode();
		ensure!(
			payload.len() < T::MaxMessagePayloadSize::get() as usize,
			SendError::MessageTooLarge
		);

		Ok(message.clone())
	}

	fn deliver(ticket: Self::Ticket) -> Result<H256, SendError> {
		// The agent_id should be same as in V1
		let asset_hub_agent_id = AgentIdOf::convert_location(
			&ParentThen(Parachain(T::AssetHubParaId::get().into()).into()).into(),
		)
		.ok_or(SendError::InvalidOrigin)?;

		let mut origin = AggregateMessageOrigin::SnowbridgeV2(ticket.origin);
		if !ticket.from_governance {
			origin = AggregateMessageOrigin::SnowbridgeV2(asset_hub_agent_id);
		}

		let message =
			BoundedVec::try_from(ticket.encode()).map_err(|_| SendError::MessageTooLarge)?;

		T::MessageQueue::enqueue_message(message.as_bounded_slice(), origin);
		Self::deposit_event(Event::MessageQueued { message: ticket.clone() });
		Ok(ticket.id)
	}
}
