// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that `CandidateReceiptV2` works correctly on a bitten Polkadot network with V3 enabled.
//!
//! Requires zombie-bite artifacts (chain specs + DB snapshots) and the `doppelganger` /
//! `doppelganger-parachain` binaries produced by `doppelganger-wrapper` pointing mchr-doppelganger-stable2603
//! branch (polkadot-sdk).
//!
//! zombie-bite builded from mchr-v3-node-feature branch
//!
//! ## Setup
//!
//! ```sh
//!  ZOMBIE_RC_RUNWAY=X zombie-bite bite -d /tmp/zombie_bites -r polkadot \
//!   --rc-override polkadot_runtime-v2002001.compact.compressed.wasm \
//!   --parachains people,collectives
//! ```
//! where X = number of slots before the next BABE epoch boundary
//!
//! ## Run
//!
//! ```sh
//! ZOMBIE_PROVIDER=native \
//! ZOMBIE_BITE_DIR=/tmp/zombie_bites/bite \
//! cargo test -p polkadot-zombienet-sdk-tests --features zombie-ci \
//!   v3_bitten_network_test -- --nocapture
//! ```
//!
//! ## What it checks
//!
//! 1. `CandidateReceiptV3` node feature (bit 4) is NOT enabled before the session change.
//! 2. Both system parachains (people 1004, collectives 1001) produce V2 candidates after the
//!    session transition.
//! 3. `CandidateReceiptV3` node feature (bit 4) is enabled after the session change.
//! 4. No disputes are raised.
//! 5. All validators sign backing statements.
//! 6. Approval checking finality lag stays below 6 on all validators.
//! 7. GRANDPA finality is not stalled on either parachain.

use crate::utils::{
	assert_candidates_version, assert_node_feature_enabled,
	assert_validator_backed_candidates,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_finality_lag;
use futures::StreamExt;
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use std::collections::HashMap;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	GlobalSettingsBuilder, LocalFileSystem, Network, NetworkConfig,
};

/// Read the zombie-bite generated `config.toml` and strip the `base_dir` line.
///
/// Mirrors what zombie-bite's `localize_config()` does: the `base_dir` is removed
/// because global settings will supply it at load time.
fn prepare_bite_config(bite_dir: &str) -> Result<String, anyhow::Error> {
	let config_path = format!("{bite_dir}/config.toml");
	let content = std::fs::read_to_string(&config_path)
		.map_err(|e| anyhow!("failed to read {config_path}: {e}"))?;

	let prepared = content
		.lines()
		.filter(|line| !line.trim_start().starts_with("base_dir"))
		.collect::<Vec<_>>()
		.join("\n");

	Ok(prepared)
}

/// Wait for all collators to be reachable and for 3 finalized relay blocks.
async fn ensure_startup_producing_blocks(network: &Network<LocalFileSystem>) {
	for para in network.parachains() {
		for collator in para.collators() {
			log::debug!("Waiting metrics for collator {}", collator.name());
			collator
				.wait_metric_with_timeout("node_roles", |x| x > 1.0, 300_u64)
				.await
				.unwrap();
		}
	}

	let client = network
		.get_node("alice")
		.unwrap()
		.wait_client::<PolkadotConfig>()
		.await
		.unwrap();
	let mut blocks = client.blocks().subscribe_finalized().await.unwrap().take(3);

	while let Some(block) = blocks.next().await {
		log::info!("Block #{}", block.unwrap().header().number);
	}

	log::info!("network is up and running");
}

#[tokio::test(flavor = "multi_thread")]
async fn v3_bitten_network_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let bite_dir =
		std::env::var("ZOMBIE_BITE_DIR").unwrap_or_else(|_| "/tmp/zombie_bites/bite".into());

	// Strip base_dir from the bite config (global settings will supply it).
	let prepared_toml = prepare_bite_config(&bite_dir)?;
	let config_path = format!("{bite_dir}/test-config.toml");
	std::fs::write(&config_path, &prepared_toml)?;
	log::info!("Wrote prepared config to {config_path}");

	// Build global settings with a base dir — same as zombie-bite's spawn().
	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR").unwrap_or_else(|_| {
		format!("{}/zombie-bitten-{}", std::env::temp_dir().display(), std::process::id())
	});
	let global_settings = GlobalSettingsBuilder::new()
		.with_base_dir(&base_dir)
		.build()
		.map_err(|e| anyhow!("global settings: {e:?}"))?;

	// Load from TOML — the same code path zombie-bite uses.
	let network_config =
		NetworkConfig::load_from_toml_with_settings(&config_path, &global_settings)
			.map_err(|e| anyhow!("load toml: {e:?}"))?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(network_config).await?;

	ensure_startup_producing_blocks(&network).await;

	let relay_node = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	let para_people = ParaId::from(1004);
	let para_collectives = ParaId::from(1001);

	log::info!("verifying V3 inode feature is NOT enabled before session change");
	assert!(assert_node_feature_enabled(&relay_client, 4).await.is_err());

	// Assert candidates on first session change.
	// On session change v3 feature will be enabled
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([
			(para_people, 10..21),
			(para_collectives, 10..21),
		]),
		30,
	)
	.await?;

	assert_node_feature_enabled(&relay_client, 4).await?;

	// Verify no disputes are raised.
	log::info!("checking no disputes");
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_disputes_total",
			|v| v == 0.0,
			30u64,
		)
		.await?;

	// Verify all validators sign backing statements.
	log::info!("checking all validators backed candidates");
	for name in ["alice", "bob", "charlie", "dave"] {
		let node = network.get_node(name)?;
		assert_validator_backed_candidates(node, 30).await?;
	}

	log::info!("checking approval checking finality lag");
	for name in ["alice", "bob", "charlie", "dave"] {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout(
			"polkadot_parachain_approval_checking_finality_lag",
			|v| v <= 6.0,
			30u64,
		)
		.await
		.map_err(|e| anyhow!("approval checking lag too high on {name}: {e}"))?;
	}

	// Verify GRANDPA finality is not stalled on any parachain.
	log::info!("checking finality lag");
	let people_node = network.get_node("Collator-1004")?;
	let collectives_node = network.get_node("Collator-1001")?;

	assert_finality_lag(&people_node.wait_client().await?, 6).await?;
	assert_finality_lag(&collectives_node.wait_client().await?, 6).await?;

	log::info!("V3 bitten network test finished successfully");

	Ok(())
}
