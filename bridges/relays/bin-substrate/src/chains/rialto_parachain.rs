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

use crate::cli::{bridge, encode_message::CliEncodeMessage, CliChain};
use bp_runtime::EncodedOrDecodedCall;
use bridge_runtime_common::CustomNetworkId;
use relay_rialto_parachain_client::RialtoParachain;
use relay_substrate_client::SimpleRuntimeVersion;
use xcm::latest::prelude::*;

impl CliEncodeMessage for RialtoParachain {
	fn encode_send_xcm(
		message: xcm::VersionedXcm<()>,
		bridge_instance_index: u8,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		type RuntimeCall = relay_rialto_parachain_client::RuntimeCall;
		type XcmCall = relay_rialto_parachain_client::runtime_types::pallet_xcm::pallet::Call;

		let dest = match bridge_instance_index {
			bridge::RIALTO_PARACHAIN_TO_MILLAU_INDEX =>
				(Parent, X1(GlobalConsensus(CustomNetworkId::Millau.as_network_id()))),
			_ => anyhow::bail!(
				"Unsupported target bridge pallet with instance index: {}",
				bridge_instance_index
			),
		};

		let xcm_call = XcmCall::send {
			dest: Box::new(unsafe { std::mem::transmute(xcm::VersionedMultiLocation::from(dest)) }),
			message: Box::new(unsafe { std::mem::transmute(message) }),
		};

		Ok(RuntimeCall::PolkadotXcm(xcm_call).into())
	}
}

impl CliChain for RialtoParachain {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> = None;
}
