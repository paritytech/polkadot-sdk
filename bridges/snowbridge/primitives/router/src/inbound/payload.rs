// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::inbound::v2::{
	Asset::{ForeignTokenERC20, NativeTokenERC20},
	Message,
};
use alloy_sol_types::{sol, SolType};
use sp_core::{RuntimeDebug, H160, H256};

sol! {
	struct AsNativeTokenERC20 {
		address token_id;
		uint128 value;
	}
}

sol! {
	struct AsForeignTokenERC20 {
		bytes32 token_id;
		uint128 value;
	}
}

sol! {
	struct EthereumAsset {
		uint8 kind;
		bytes data;
	}
}

sol! {
	struct Payload {
		address origin;
		EthereumAsset[] assets;
		bytes xcm;
		bytes claimer;
		uint128 value;
		uint128 executionFee;
		uint128 relayerFee;
	}
}

#[derive(Copy, Clone, RuntimeDebug)]
pub struct PayloadDecodeError;
impl TryFrom<&[u8]> for Message {
	type Error = PayloadDecodeError;

	fn try_from(encoded_payload: &[u8]) -> Result<Self, Self::Error> {
		let decoded_payload =
			Payload::abi_decode(&encoded_payload, true).map_err(|_| PayloadDecodeError)?;

		let mut substrate_assets = vec![];

		for asset in decoded_payload.assets {
			match asset.kind {
				0 => {
					let native_data = AsNativeTokenERC20::abi_decode(&asset.data, true)
						.map_err(|_| PayloadDecodeError)?;
					substrate_assets.push(NativeTokenERC20 {
						token_id: H160::from(native_data.token_id.as_ref()),
						value: native_data.value,
					});
				},
				1 => {
					let foreign_data = AsForeignTokenERC20::abi_decode(&asset.data, true)
						.map_err(|_| PayloadDecodeError)?;
					substrate_assets.push(ForeignTokenERC20 {
						token_id: H256::from(foreign_data.token_id.as_ref()),
						value: foreign_data.value,
					});
				},
				_ => return Err(PayloadDecodeError),
			}
		}

		let mut claimer = None;
		if decoded_payload.claimer.len() > 0 {
			claimer = Some(decoded_payload.claimer);
		}

		Ok(Self {
			origin: H160::from(decoded_payload.origin.as_ref()),
			assets: substrate_assets,
			xcm: decoded_payload.xcm,
			claimer,
			value: decoded_payload.value,
			execution_fee: decoded_payload.executionFee,
			relayer_fee: decoded_payload.relayerFee,
		})
	}
}
