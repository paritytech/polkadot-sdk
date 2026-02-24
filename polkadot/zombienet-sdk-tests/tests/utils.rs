// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use zombienet_orchestrator::tx_helper::parachain::{fetch_genesis_header, fetch_validation_code};
use zombienet_sdk::{
	subxt::{dynamic::Value, ext::scale_value::value, tx, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	LocalFileSystem, Network, NetworkConfig,
};

pub const PARACHAIN_VALIDATOR_METRIC: &str = "polkadot_node_is_parachain_validator";
pub const ACTIVE_VALIDATOR_METRIC: &str = "polkadot_node_is_active_validator";
pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const CUMULUS_IMAGE_ENV: &str = "CUMULUS_IMAGE";
pub const COL_IMAGE_ENV: &str = "COL_IMAGE";
pub const DISPUTE_CONCLUDED_INVALID_METRIC: &str =
	"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";
pub const APPROVALS_NO_SHOWS_TOTAL_METRIC: &str = "polkadot_parachain_approvals_no_shows_total";

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

pub fn env_or_default(var: &str, default: &str) -> String {
	std::env::var(var).unwrap_or_else(|_| default.to_string())
}
