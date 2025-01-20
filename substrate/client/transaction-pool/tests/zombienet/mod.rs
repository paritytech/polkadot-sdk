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

// Integration tests for fork-aware transaction pool.

use std::path::PathBuf;

use derive_builder::Builder;
use zombienet_configuration::{shared::types::Arg, types::ParaId};
use zombienet_sdk::{LocalFileSystem, Network as ZNetwork, NetworkConfig};

pub mod limits_30;
pub mod old_pool;
pub mod old_pool_small;
pub mod single_collator;
pub mod yap;

const DEFAULT_BASE_DIR: &'static str = "/tmp/zn-spawner";
const DEFAULT_RC_NODE_RPC_PORT: u16 = 9944;
const DEFAULT_PC_NODE_RPC_PORT: u16 = 8844;

#[derive(Default, Builder, Debug)]
pub struct RelaychainConfig {
	default_command: String,
	chain: String,
}

impl RelaychainConfig {
	pub fn new(default_command: String, chain: String) -> Self {
		RelaychainConfig { default_command, chain }
	}
}

#[derive(Default, Builder, Debug)]
pub struct ParachainConfig {
	default_command: String,
	chain_spec_path: String,
	cumulus_based: bool,
	pub id: ParaId,
}

impl ParachainConfig {
	pub fn new(
		default_command: String,
		chain_spec_path: String,
		cumulus_based: bool,
		id: ParaId,
	) -> Self {
		ParachainConfig { default_command, chain_spec_path, cumulus_based, id }
	}
}

/// Wrapper over a substrate node managed by zombienet.
#[derive(Debug)]
pub struct Node {
	validator: bool,
	name: String,
	args: Vec<Arg>,
}

impl Node {
	pub fn new(name: String, args: Vec<Arg>, validator: bool) -> Self {
		Node { name, args, validator }
	}
}

#[async_trait::async_trait]
pub trait Network {
	// Ensure the necesary bins are on $PATH.
	fn ensure_bins_on_path(&self) -> bool;

	// Provide zombienet network config.
	fn config(&self) -> Result<NetworkConfig, anyhow::Error>;

	// Start the network locally.
	async fn start(&self) -> Result<ZNetwork<LocalFileSystem>, anyhow::Error>;

	// Return filesystem base dir of the network with all relevant files (e.g. logs).
	fn base_dir(&self) -> &PathBuf;
}
