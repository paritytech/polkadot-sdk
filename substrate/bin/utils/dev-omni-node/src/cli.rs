// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The CLI parameters of the dev omni-node.

use sc_cli::RunCmd;

/// The consensus algorithm to use.
#[derive(Debug, Clone)]
pub enum Consensus {
	/// Manual seal, with the block time in milliseconds.
	///
	/// Should be provided as `manual-seal-3000` for a 3 seconds block time.
	ManualSeal(u64),
	/// Instant seal.
	///
	/// Authors a new block as soon as a transaction is received.
	InstantSeal,
}

impl std::str::FromStr for Consensus {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(if s == "instant-seal" {
			Consensus::InstantSeal
		} else if let Some(block_time) = s.strip_prefix("manual-seal-") {
			Consensus::ManualSeal(block_time.parse().map_err(|_| "invalid block time")?)
		} else {
			return Err("incorrect consensus identifier".into())
		})
	}
}

/// You typically run this node with:
///
/// * Either of `--chain runtime.wasm` or `--chain spec.json`.
/// * `--tmp` to use a temporary database.
///
/// This binary goes hand in hand with:
///
/// * a `.wasm` file. You typically get this from your runtime template.
/// * `chain-spec-builder` to generate chain-spec. You might possibly edit this chain-spec manually,
///   or alter your runtime's `sp_genesis_builder` impl with specific presets.
///
/// * `frame-omni-bencher` to create benchmarking.
#[derive(Debug, clap::Parser)]
pub struct Cli {
	/// The subcommand to use.
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	/// The block authoring (aka. consensus) engine to use.
	#[clap(long, default_value = "manual-seal-1000")]
	pub consensus: Consensus,

	/// The main run command
	#[clap(flatten)]
	pub run: RunCmd,
}

/// Possible sub-commands.
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}
