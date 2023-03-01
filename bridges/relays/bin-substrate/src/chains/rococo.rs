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

//! Rococo + Rococo parachains specification for CLI.

use crate::cli::CliChain;
use relay_bridge_hub_rococo_client::BridgeHubRococo;
use relay_rococo_client::Rococo;
use relay_substrate_client::SimpleRuntimeVersion;

impl CliChain for Rococo {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> = None;
}

impl CliChain for BridgeHubRococo {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion { spec_version: 9372, transaction_version: 1 });
}
