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

use crate::error::Result;
use clap::Parser;
use sc_service::{chain_ops, ChainSpec};
use std::{
	fs,
	io::{self, Write},
	path::PathBuf,
};

/// Export a chain-spec to a JSON file in plain or in raw storage format.
///
/// Nodes that expose this command usually have embedded runtimes WASM blobs with
/// genesis config presets which can be referenced via `--chain <id>` . The logic for
/// loading the chain spec into memory based on an `id` is specific to each
/// node and is a prerequisite to enable this command.
#[derive(Debug, Clone, Parser)]
pub struct ExportChainSpecCmd {
	/// The chain spec identifier to export.
	#[arg(long, default_value = "")]
	pub chain: String,

	/// `chain-spec` JSON file path. If omitted, prints to stdout.
	#[arg(long)]
	pub output: Option<PathBuf>,

	/// Export in raw genesis storage format.
	#[arg(long)]
	pub raw: bool,
}

impl ExportChainSpecCmd {
	/// Run the export-chain-spec command
	pub fn run(&self, spec: Box<dyn ChainSpec>) -> Result<()> {
		let json = chain_ops::build_spec(spec.as_ref(), self.raw)?;
		if let Some(ref path) = self.output {
			fs::write(path, json)?;
			println!("Exported chain spec to {}", path.display());
		} else {
			io::stdout().write_all(json.as_bytes()).map_err(|e| format!("{}", e))?;
		}
		Ok(())
	}
}
