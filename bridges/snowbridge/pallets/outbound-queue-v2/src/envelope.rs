// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use snowbridge_core::inbound::Log;

use sp_core::{RuntimeDebug, H160};
use sp_std::prelude::*;

use alloy_primitives::B256;
use alloy_sol_types::{sol, SolEvent};

sol! {
	event InboundMessageDispatched(uint64 indexed nonce, bool success, bytes32 reward_address);
}

/// An inbound message that has had its outer envelope decoded.
#[derive(Clone, RuntimeDebug)]
pub struct Envelope {
	/// The address of the outbound queue on Ethereum that emitted this message as an event log
	pub gateway: H160,
	/// A nonce for enforcing replay protection and ordering.
	pub nonce: u64,
	/// Delivery status
	pub success: bool,
	/// The reward address
	pub reward_address: [u8; 32],
}

#[derive(Copy, Clone, RuntimeDebug)]
pub struct EnvelopeDecodeError;

impl TryFrom<&Log> for Envelope {
	type Error = EnvelopeDecodeError;

	fn try_from(log: &Log) -> Result<Self, Self::Error> {
		let topics: Vec<B256> = log.topics.iter().map(|x| B256::from_slice(x.as_ref())).collect();

		let event = InboundMessageDispatched::decode_log(topics, &log.data, true)
			.map_err(|_| EnvelopeDecodeError)?;

		Ok(Self {
			gateway: log.address,
			nonce: event.nonce,
			success: event.success,
			reward_address: event.reward_address.clone().into(),
		})
	}
}
