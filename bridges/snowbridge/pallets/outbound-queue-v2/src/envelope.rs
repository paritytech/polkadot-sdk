// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use snowbridge_core::inbound::Log;

use sp_core::{RuntimeDebug, H160};
use sp_std::prelude::*;

use crate::Config;
use alloy_core::{primitives::B256, sol, sol_types::SolEvent};
use codec::Decode;
use frame_support::pallet_prelude::{Encode, TypeInfo};

sol! {
	event InboundMessageDispatched(uint64 indexed nonce, bool success, bytes32 reward_address);
}

/// An inbound message that has had its outer envelope decoded.
#[derive(Clone, RuntimeDebug)]
pub struct Envelope<T: Config> {
	/// The address of the outbound queue on Ethereum that emitted this message as an event log
	pub gateway: H160,
	/// A nonce for enforcing replay protection and ordering.
	pub nonce: u64,
	/// Delivery status
	pub success: bool,
	/// The reward address
	pub reward_address: T::AccountId,
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum EnvelopeDecodeError {
	DecodeLogFailed,
	DecodeAccountFailed,
}

impl<T: Config> TryFrom<&Log> for Envelope<T> {
	type Error = EnvelopeDecodeError;

	fn try_from(log: &Log) -> Result<Self, Self::Error> {
		let topics: Vec<B256> = log.topics.iter().map(|x| B256::from_slice(x.as_ref())).collect();

		let event = InboundMessageDispatched::decode_raw_log(topics, &log.data, true)
			.map_err(|_| EnvelopeDecodeError::DecodeLogFailed)?;

		let account = T::AccountId::decode(&mut &event.reward_address[..])
			.map_err(|_| EnvelopeDecodeError::DecodeAccountFailed)?;

		Ok(Self {
			gateway: log.address,
			nonce: event.nonce,
			success: event.success,
			reward_address: account,
		})
	}
}
