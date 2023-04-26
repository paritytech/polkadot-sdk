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

use crate::cli::{encode_message::CliEncodeMessage, CliChain};
use bp_runtime::EncodedOrDecodedCall;
use relay_rialto_parachain_client::RialtoParachain;
use relay_substrate_client::SimpleRuntimeVersion;

impl CliEncodeMessage for RialtoParachain {
	fn encode_execute_xcm(
		message: xcm::VersionedXcm<Self::Call>,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>> {
		type RuntimeCall = relay_rialto_parachain_client::RuntimeCall;
		type XcmCall = relay_rialto_parachain_client::runtime_types::pallet_xcm::pallet::Call;

		let xcm_call = XcmCall::execute {
			message: Box::new(unsafe { std::mem::transmute(message) }),
			max_weight: Self::estimate_execute_xcm_weight(),
		};

		Ok(RuntimeCall::PolkadotXcm(xcm_call).into())
	}
}

impl CliChain for RialtoParachain {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> = None;
}
