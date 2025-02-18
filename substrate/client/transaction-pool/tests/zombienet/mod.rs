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

//! The zombienet spawner for integration tests for a transaction pool. Holds shared logic used
//! across integration tests for transaction pool.

use anyhow::anyhow;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig, NetworkConfigExt};

pub const ASSET_HUB_LOW_POOL_LIMIT_FATP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-low-pool-limit-fatp.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_FATP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-fatp.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_OLDP_3_COLLATORS_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-oldp-3-collators.toml";
pub const ASSET_HUB_HIGH_POOL_LIMIT_OLDP_4_COLLATORS_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/asset-hub-high-pool-limit-oldp-4-collators.toml";

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("Network initialization failure: {0}")]
	NetworkInit(anyhow::Error),
}

type Result<T> = std::result::Result<T, Error>;

/// Provides logic to spawn a network based on a Zombienet toml file.
pub struct NetworkSpawner;

impl NetworkSpawner {
	pub async fn from_toml(toml_path: &'static str) -> Result<Network<LocalFileSystem>> {
		let net_config = NetworkConfig::load_from_toml(toml_path).map_err(Error::NetworkInit)?;
		net_config
			.spawn_native()
			.await
			.map_err(|err| Error::NetworkInit(anyhow!(err.to_string())))
	}
}
