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

//! Wococo + Wococo parachains specification for CLI.

use crate::cli::CliChain;
use relay_bridge_hub_wococo_client::BridgeHubWococo;
use relay_wococo_client::Wococo;
use sp_version::RuntimeVersion;

impl CliChain for Wococo {
	const RUNTIME_VERSION: Option<RuntimeVersion> = None;

	type KeyPair = sp_core::sr25519::Pair;

	fn ss58_format() -> u16 {
		bp_wococo::SS58Prefix::get() as u16
	}
}

impl CliChain for BridgeHubWococo {
	const RUNTIME_VERSION: Option<RuntimeVersion> = None;

	type KeyPair = sp_core::sr25519::Pair;

	fn ss58_format() -> u16 {
		relay_bridge_hub_wococo_client::runtime::SS58Prefix::get()
	}
}
