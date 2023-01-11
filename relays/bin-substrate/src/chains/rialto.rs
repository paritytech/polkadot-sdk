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

//! Rialto chain specification for CLI.

use crate::cli::{bridge, encode_message::CliEncodeMessage, CliChain};
use bp_runtime::EncodedOrDecodedCall;
use relay_rialto_client::Rialto;
use relay_substrate_client::SimpleRuntimeVersion;
use xcm::latest::prelude::*;

impl CliEncodeMessage for Rialto {
	fn encode_send_xcm(
		message: xcm::VersionedXcm<()>,
		bridge_instance_index: u8,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		let dest = match bridge_instance_index {
			bridge::RIALTO_TO_MILLAU_INDEX =>
				(Parent, X1(GlobalConsensus(rialto_runtime::xcm_config::MillauNetwork::get()))),
			_ => anyhow::bail!(
				"Unsupported target bridge pallet with instance index: {}",
				bridge_instance_index
			),
		};

		Ok(rialto_runtime::RuntimeCall::XcmPallet(rialto_runtime::XcmCall::send {
			dest: Box::new(dest.into()),
			message: Box::new(message),
		})
		.into())
	}
}

impl CliChain for Rialto {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion::from_runtime_version(&rialto_runtime::VERSION));
}
