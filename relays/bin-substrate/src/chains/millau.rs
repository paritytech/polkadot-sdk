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

use crate::cli::{encode_message::CliEncodeMessage, CliChain};
use bp_runtime::EncodedOrDecodedCall;
use bridge_runtime_common::CustomNetworkId;
use relay_millau_client::Millau;
use relay_substrate_client::SimpleRuntimeVersion;
use xcm_executor::traits::ExportXcm;

impl CliEncodeMessage for Millau {
	fn encode_wire_message(
		target: xcm::v3::NetworkId,
		at_target_xcm: xcm::v3::Xcm<()>,
	) -> anyhow::Result<Vec<u8>> {
		anyhow::ensure!(
			[
				CustomNetworkId::Rialto.as_network_id(),
				CustomNetworkId::RialtoParachain.as_network_id()
			]
			.contains(&target),
			anyhow::format_err!("Unsupported target chain: {:?}", target)
		);

		Ok(millau_runtime::xcm_config::ToRialtoOrRialtoParachainSwitchExporter::validate(
			target,
			0,
			&mut Some(Self::dummy_universal_source()?),
			&mut Some(target.into()),
			&mut Some(at_target_xcm),
		)
		.map_err(|e| anyhow::format_err!("Failed to prepare outbound message: {:?}", e))?
		.0
		 .1
		 .0)
	}

	fn encode_execute_xcm(
		message: xcm::VersionedXcm<Self::Call>,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		Ok(millau_runtime::RuntimeCall::XcmPallet(millau_runtime::XcmCall::execute {
			message: Box::new(message),
			max_weight: Self::estimate_execute_xcm_weight(),
		})
		.into())
	}
}

impl CliChain for Millau {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion::from_runtime_version(&millau_runtime::VERSION));
}
