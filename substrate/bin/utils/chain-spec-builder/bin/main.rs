// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use chain_spec_builder::{
	generate_chain_spec_for_runtime, ChainSpecBuilder, ChainSpecBuilderCmd, ConvertToRawCmd,
	UpdateCodeCmd, VerifyCmd,
};
use clap::Parser;
use sc_chain_spec::{update_code_in_json_chain_spec, GenericChainSpec};
use staging_chain_spec_builder as chain_spec_builder;
use std::fs;

fn main() -> Result<(), String> {
	sp_tracing::try_init_simple();

	let builder = ChainSpecBuilder::parse();
	let chain_spec_path = builder.chain_spec_path.to_path_buf();

	match builder.command {
		ChainSpecBuilderCmd::Create(cmd) => {
			let chain_spec_json = generate_chain_spec_for_runtime(&cmd)?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::UpdateCode(UpdateCodeCmd {
			ref input_chain_spec,
			ref runtime_wasm_path,
		}) => {
			let chain_spec = GenericChainSpec::<()>::from_json_file(input_chain_spec.clone())?;

			let mut chain_spec_json =
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(false)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;
			update_code_in_json_chain_spec(
				&mut chain_spec_json,
				&fs::read(runtime_wasm_path.as_path())
					.map_err(|e| format!("Wasm blob file could not be read: {e}"))?[..],
			);

			let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
				.map_err(|e| format!("to pretty failed: {e}"))?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::ConvertToRaw(ConvertToRawCmd { ref input_chain_spec }) => {
			let chain_spec = GenericChainSpec::<()>::from_json_file(input_chain_spec.clone())?;

			let chain_spec_json =
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;

			let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
				.map_err(|e| format!("Conversion to pretty failed: {e}"))?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::Verify(VerifyCmd { ref input_chain_spec }) => {
			let chain_spec = GenericChainSpec::<()>::from_json_file(input_chain_spec.clone())?;
			let _ = serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
				.map_err(|e| format!("Conversion to json failed: {e}"))?;
		},
	};
	Ok(())
}
