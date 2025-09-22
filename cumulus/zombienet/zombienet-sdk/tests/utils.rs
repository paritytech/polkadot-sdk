// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use cumulus_zombienet_sdk_helpers::{
	create_assign_core_call, submit_extrinsic_and_wait_for_finalization_success_with_timeout,
};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	LocalFileSystem, Network, NetworkConfig, NetworkNode,
};

pub const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

pub async fn initialize_network(
	config: NetworkConfig,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	// Spawn network
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	// Do not terminate network after the test is finished.
	// This is needed for CI to get logs from k8s.
	// Network shall be terminated from CI after logs are downloaded.
	// NOTE! For local execution (native provider) below call has no effect.
	network.detach().await;

	Ok(network)
}

pub async fn assign_cores(
	node: &NetworkNode,
	para_id: u32,
	cores: Vec<u32>,
) -> Result<(), anyhow::Error> {
	log::info!("Assigning {:?} cores to parachain {}", cores, para_id);

	let assign_cores_call =
		create_assign_core_call(&cores.into_iter().map(|core| (core, para_id)).collect::<Vec<_>>());

	let client: OnlineClient<PolkadotConfig> = node.wait_client().await?;
	let res = submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		&client,
		&assign_cores_call,
		&zombienet_sdk::subxt_signer::sr25519::dev::alice(),
		60u64,
	)
	.await;
	assert!(res.is_ok(), "Extrinsic failed to finalize: {:?}", res.unwrap_err());
	log::info!("Cores assigned to the parachain");

	Ok(())
}
