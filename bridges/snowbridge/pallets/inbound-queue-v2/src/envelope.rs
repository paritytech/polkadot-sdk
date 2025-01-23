// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use snowbridge_core::inbound::Log;

use alloy_core::{
	primitives::B256,
	sol,
	sol_types::{SolEvent, SolType},
};
use snowbridge_router_primitives::inbound::v2::{
	Asset::{ForeignTokenERC20, NativeTokenERC20},
	Message as MessageV2,
};
use sp_core::{RuntimeDebug, H160, H256};
use sp_std::prelude::*;
sol! {
	struct AsNativeTokenERC20 {
		address token_id;
		uint128 value;
	}
	struct AsForeignTokenERC20 {
		bytes32 token_id;
		uint128 value;
	}
	struct EthereumAsset {
		uint8 kind;
		bytes data;
	}
	struct Payload {
		address origin;
		EthereumAsset[] assets;
		bytes xcm;
		bytes claimer;
		uint128 value;
		uint128 executionFee;
		uint128 relayerFee;
	}
	event OutboundMessageAccepted(uint64 nonce, Payload payload);
}

/// An inbound message that has had its outer envelope decoded.
#[derive(Clone, RuntimeDebug)]
pub struct Envelope {
	/// The address of the outbound queue on Ethereum that emitted this message as an event log
	pub gateway: H160,
	/// A nonce for enforcing replay protection and ordering.
	pub nonce: u64,
	/// The inner payload generated from the source application.
	pub message: MessageV2,
}

#[derive(Copy, Clone, RuntimeDebug)]
pub struct EnvelopeDecodeError;

impl TryFrom<&Log> for Envelope {
	type Error = EnvelopeDecodeError;

	fn try_from(log: &Log) -> Result<Self, Self::Error> {
		// Convert to B256 for Alloy decoding
		let topics: Vec<B256> = log.topics.iter().map(|x| B256::from_slice(x.as_ref())).collect();

		let mut substrate_assets = alloc::vec![];

		// Decode the Solidity event from raw logs
		let event = OutboundMessageAccepted::decode_raw_log(topics, &log.data, true).map_err(
			|decode_err| {
				log::error!(
					target: "snowbridge-inbound-queue:v2",
					"💫 decode error {:?}",
					decode_err
				);
				EnvelopeDecodeError
			},
		)?;

		let payload = event.payload;

		for asset in payload.assets {
			match asset.kind {
				0 => {
					let native_data = AsNativeTokenERC20::abi_decode(&asset.data, true)
						.map_err(|_| EnvelopeDecodeError)?;
					substrate_assets.push(NativeTokenERC20 {
						token_id: H160::from(native_data.token_id.as_ref()),
						value: native_data.value,
					});
				},
				1 => {
					let foreign_data = AsForeignTokenERC20::abi_decode(&asset.data, true)
						.map_err(|_| EnvelopeDecodeError)?;
					substrate_assets.push(ForeignTokenERC20 {
						token_id: H256::from(foreign_data.token_id.as_ref()),
						value: foreign_data.value,
					});
				},
				_ => return Err(EnvelopeDecodeError),
			}
		}

		let mut claimer = None;
		if payload.claimer.len() > 0 {
			claimer = Some(payload.claimer.to_vec());
		}

		let message = MessageV2 {
			origin: H160::from(payload.origin.as_ref()),
			assets: substrate_assets,
			xcm: payload.xcm.to_vec(),
			claimer,
			value: payload.value,
			execution_fee: payload.executionFee,
			relayer_fee: payload.relayerFee,
		};

		Ok(Self { gateway: log.address, nonce: event.nonce, message })
	}
}

impl core::fmt::Debug for Payload {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Payload")
			.field("origin", &self.origin)
			.field("assets", &self.assets)
			.field("xcm", &self.xcm)
			.field("claimer", &self.claimer)
			.field("value", &self.value)
			.field("executionFee", &self.executionFee)
			.field("relayerFee", &self.relayerFee)
			.finish()
	}
}

impl core::fmt::Debug for EthereumAsset {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("EthereumAsset")
			.field("kind", &self.kind)
			.field("data", &self.data)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use crate::{envelope::Log, Envelope};
	use frame_support::assert_ok;
	use hex_literal::hex;
	use sp_core::H160;

	#[test]
	fn test_decode() {
		let log = Log{
			address: hex!("b8ea8cb425d85536b158d661da1ef0895bb92f1d").into(),
			topics: vec![hex!("b61699d45635baed7500944331ea827538a50dbfef79180f2079e9185da627aa").into()],
			data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000001dcd6500000000000000000000000000000000000000000000000000000000003b9aca000000000000000000000000000000000000000000000000000000000059682f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cdeadbeef774667629726ec1fabebcec0d9139bd1c8f72a23deadbeef0000000000000000000000001dcd650000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").to_vec(),
		};

		let result = Envelope::try_from(&log);
		assert_ok!(result.clone());
		let envelope = result.unwrap();

		assert_eq!(H160::from(hex!("b8ea8cb425d85536b158d661da1ef0895bb92f1d")), envelope.gateway);
		assert_eq!(
			H160::from(hex!("B8EA8cB425d85536b158d661da1ef0895Bb92F1D")),
			envelope.message.origin
		);
	}
}
