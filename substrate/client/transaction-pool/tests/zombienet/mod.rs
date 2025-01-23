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

use anyhow::anyhow;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig, NetworkConfigExt};

const DEFAULT_BASE_DIR: &'static str = "/tmp/zn-spawner";

const YAP_HIGH_POOL_LIMIT_OLDP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/yap-high-pool-limit-oldp.toml";
const YAP_HIGH_POOL_LIMIT_FATP_SPEC_PATH: &'static str =
	"tests/zombienet/network-specs/yap-high-pool-limit-fatp.toml";

#[derive(thiserror::Error, Debug)]
enum Error {
	#[error("Network initialization failure: {0}")]
	NetworkInit(anyhow::Error),
}

type Result<T> = std::result::Result<T, Error>;

struct NetworkSpawner;

impl NetworkSpawner {
	async fn init_from_yap_oldp_high_pool_limit_spec() -> Result<Network<LocalFileSystem>> {
		let net_config = NetworkConfig::load_from_toml(YAP_HIGH_POOL_LIMIT_OLDP_SPEC_PATH)
			.map_err(Error::NetworkInit)?;
		net_config
			.spawn_native()
			.await
			.map_err(|err| Error::NetworkInit(anyhow!(err.to_string())))
	}

	async fn init_from_yap_fatp_high_pool_limit_spec() -> Result<Network<LocalFileSystem>> {
		let net_config = NetworkConfig::load_from_toml(YAP_GHIGH_POOL_LIMIT_FATP_SPEC_PATH)
			.map_err(Error::NetworkInit)?;
		net_config
			.spawn_native()
			.await
			.map_err(|err| Error::NetworkInit(anyhow!(err.to_string())))
	}
}
