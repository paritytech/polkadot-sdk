// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig};

pub async fn initialize_network(
	config: NetworkConfig,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// Do not terminate network after the test is finished.
	// This is needed for CI to get logs from k8s.
	// Network shall be terminated from CI after logs are downloaded.
	// NOTE! For local execution (native provider) below call has no effect.
	network.detach();

	Ok(network)
}
