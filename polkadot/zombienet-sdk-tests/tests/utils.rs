// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use zombienet_sdk::{
	subxt::{ext::scale_value::value, tx::dynamic, OnlineClient, PolkadotConfig},
	LocalFileSystem, Network, NetworkConfig,
};

pub const PARACHAIN_VALIDATOR_METRIC: &str = "polkadot_node_is_parachain_validator";
pub const ACTIVE_VALIDATOR_METRIC: &str = "polkadot_node_is_active_validator";
pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const CUMULUS_IMAGE_ENV: &str = "CUMULUS_IMAGE";
pub const COL_IMAGE_ENV: &str = "COL_IMAGE";

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

/// Enables `CandidateReceiptV3` (bit 4) in `node_features` at runtime via a sudo extrinsic.
///
/// Bit 3 (`CandidateReceiptV2`) is already set via genesis; this sets bit 4.
/// The change takes effect after the next session change.
pub async fn enable_v3_node_features(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	// set_node_feature(index, value) sets a single bit in node_features.
	// Bit 4 = CandidateReceiptV3.
	let call = dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Configuration(set_node_feature { index: 4u8, value: true })
		}],
	);

	client
		.tx()
		.sign_and_submit_then_watch_default(
			&call,
			&zombienet_sdk::subxt_signer::sr25519::dev::alice(),
		)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(())
}
