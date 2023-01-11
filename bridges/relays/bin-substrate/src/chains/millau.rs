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

//! Millau chain specification for CLI.

use crate::cli::{bridge, encode_message::CliEncodeMessage, CliChain};
use bp_rialto_parachain::RIALTO_PARACHAIN_ID;
use bp_runtime::EncodedOrDecodedCall;
use relay_millau_client::Millau;
use relay_substrate_client::SimpleRuntimeVersion;
use xcm::latest::prelude::*;

impl CliEncodeMessage for Millau {
	fn encode_send_xcm(
		message: xcm::VersionedXcm<()>,
		bridge_instance_index: u8,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		let dest = match bridge_instance_index {
			bridge::MILLAU_TO_RIALTO_INDEX =>
				(Parent, X1(GlobalConsensus(millau_runtime::xcm_config::RialtoNetwork::get()))),
			bridge::MILLAU_TO_RIALTO_PARACHAIN_INDEX => (
				Parent,
				X2(
					GlobalConsensus(millau_runtime::xcm_config::RialtoNetwork::get()),
					Parachain(RIALTO_PARACHAIN_ID),
				),
			),
			_ => anyhow::bail!(
				"Unsupported target bridge pallet with instance index: {}",
				bridge_instance_index
			),
		};

		Ok(millau_runtime::RuntimeCall::XcmPallet(millau_runtime::XcmCall::send {
			dest: Box::new(dest.into()),
			message: Box::new(message),
		})
		.into())
	}
}

impl CliChain for Millau {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion::from_runtime_version(&millau_runtime::VERSION));
}
