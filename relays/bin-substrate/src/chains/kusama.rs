// Copyright 2022 Parity Technologies (UK) Ltd.
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

//! Kusama + Kusama parachains specification for CLI.

use crate::cli::CliChain;
use relay_bridge_hub_kusama_client::BridgeHubKusama;
use relay_kusama_client::Kusama;
use relay_substrate_client::SimpleRuntimeVersion;

impl CliChain for Kusama {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> = None;
}

impl CliChain for BridgeHubKusama {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion { spec_version: 9410, transaction_version: 21 });
}
