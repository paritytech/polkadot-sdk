// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use snowbridge_core::inbound::Log;

use sp_core::{RuntimeDebug, H160, H256};
use sp_std::prelude::*;

use alloy_primitives::B256;
use alloy_sol_types::{sol, SolEvent};

sol! {
	event OutboundMessageAccepted(uint64 indexed nonce, bytes32 indexed message_id, uint32 indexed para_id, bytes32 reward_address, uint128 fee, bytes payload);
}

/// An inbound message that has had its outer envelope decoded.
#[derive(Clone, RuntimeDebug)]
pub struct Envelope {
	/// The address of the outbound queue on Ethereum that emitted this message as an event log
	pub gateway: H160,
	/// A nonce for enforcing replay protection and ordering.
	pub nonce: u64,
	/// An id for tracing the message on its route (has no role in bridge consensus)
	pub message_id: H256,
	/// Destination ParaId
	pub para_id: u32,
	/// The reward address
	pub reward_address: [u8; 32],
	/// Total fee paid on source chain
	pub fee: u128,
	/// The inner payload generated from the source application.
	pub payload: Vec<u8>,
}

#[derive(Copy, Clone, RuntimeDebug)]
pub struct EnvelopeDecodeError;

impl TryFrom<&Log> for Envelope {
	type Error = EnvelopeDecodeError;

	fn try_from(log: &Log) -> Result<Self, Self::Error> {
		let topics: Vec<B256> = log.topics.iter().map(|x| B256::from_slice(x.as_ref())).collect();

		let event = OutboundMessageAccepted::decode_log(topics, &log.data, true)
			.map_err(|_| EnvelopeDecodeError)?;

		Ok(Self {
			gateway: log.address,
			nonce: event.nonce,
			message_id: H256::from(event.message_id.as_ref()),
			reward_address: event.reward_address.clone().into(),
			fee: event.fee,
			para_id: event.para_id,
			payload: event.payload,
		})
	}
}
