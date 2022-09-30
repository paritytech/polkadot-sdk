// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Rialto parachain specification for CLI.

use crate::cli::{
	bridge,
	encode_message::{CliEncodeMessage, RawMessage},
	CliChain,
};
use bp_messages::LaneId;
use bp_runtime::EncodedOrDecodedCall;
use relay_rialto_parachain_client::RialtoParachain;
use relay_substrate_client::BalanceOf;
use sp_version::RuntimeVersion;
use xcm::latest::prelude::*;

impl CliEncodeMessage for RialtoParachain {
	fn encode_send_xcm(
		message: xcm::VersionedXcm<()>,
		bridge_instance_index: u8,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		let dest = match bridge_instance_index {
			bridge::RIALTO_PARACHAIN_TO_MILLAU_INDEX =>
				(Parent, X1(GlobalConsensus(rialto_parachain_runtime::MillauNetwork::get()))),
			_ => anyhow::bail!(
				"Unsupported target bridge pallet with instance index: {}",
				bridge_instance_index
			),
		};

		Ok(rialto_parachain_runtime::Call::PolkadotXcm(rialto_parachain_runtime::XcmCall::send {
			dest: Box::new(dest.into()),
			message: Box::new(message),
		})
		.into())
	}

	fn encode_send_message_call(
		lane: LaneId,
		payload: RawMessage,
		fee: BalanceOf<Self>,
		bridge_instance_index: u8,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		Ok(match bridge_instance_index {
			bridge::RIALTO_PARACHAIN_TO_MILLAU_INDEX =>
				rialto_parachain_runtime::Call::BridgeMillauMessages(
					rialto_parachain_runtime::MessagesCall::send_message {
						lane_id: lane,
						payload,
						delivery_and_dispatch_fee: fee,
					},
				)
				.into(),
			_ => anyhow::bail!(
				"Unsupported target bridge pallet with instance index: {}",
				bridge_instance_index
			),
		})
	}
}

impl CliChain for RialtoParachain {
	const RUNTIME_VERSION: Option<RuntimeVersion> = Some(rialto_parachain_runtime::VERSION);

	type KeyPair = sp_core::sr25519::Pair;
	type MessagePayload = Vec<u8>;

	fn ss58_format() -> u16 {
		rialto_parachain_runtime::SS58Prefix::get() as u16
	}
}
