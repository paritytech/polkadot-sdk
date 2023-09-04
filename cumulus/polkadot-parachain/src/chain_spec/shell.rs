// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::chain_spec::Extensions;
use cumulus_primitives_core::ParaId;
use sc_service::ChainType;

/// Specialized `ChainSpec` for the shell parachain runtime.
pub type ShellChainSpec = sc_service::GenericChainSpec<(), Extensions>;

pub fn get_shell_chain_spec() -> ShellChainSpec {
	ShellChainSpec::builder(
		shell_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
	.with_name("Shell Local Testnet")
	.with_id("shell_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(serde_json::json!({
		"parachainInfo": { "parachainId": ParaId::from(1000) }
	}))
	.with_boot_nodes(Vec::new())
	.build()
}
