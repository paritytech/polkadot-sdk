// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::Log;
use alloy_core::{primitives::B256, sol, sol_types::SolEvent};
use codec::Decode;
use frame_support::pallet_prelude::{Encode, TypeInfo};
use sp_core::{RuntimeDebug, H160};
use sp_std::prelude::*;

sol! {
	event InboundMessageDispatched(uint64 indexed nonce, bool success, bytes32 reward_address);
}

/// Delivery receipt
#[derive(Clone, RuntimeDebug)]
pub struct DeliveryReceipt {
	/// The address of the outbound queue on Ethereum that emitted this message as an event log
	pub gateway: H160,
	/// The nonce of the dispatched message
	pub nonce: u64,
	/// Delivery status
	pub success: bool,
	/// The reward address
	pub reward_address: [u8; 32],
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum DeliveryReceiptDecodeError {
	DecodeLogFailed,
	DecodeAccountFailed,
}

impl TryFrom<&Log> for DeliveryReceipt {
	type Error = DeliveryReceiptDecodeError;

	fn try_from(log: &Log) -> Result<Self, Self::Error> {
		let topics: Vec<B256> = log.topics.iter().map(|x| B256::from_slice(x.as_ref())).collect();

		let event = InboundMessageDispatched::decode_raw_log(topics, &log.data, true)
			.map_err(|_| DeliveryReceiptDecodeError::DecodeLogFailed)?;

		Ok(Self {
			gateway: log.address,
			nonce: event.nonce,
			success: event.success,
			reward_address: event.reward_address.0,
		})
	}
}
