// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{dynamic::Value, ext::scale_value::value, OnlineClient, PolkadotConfig},
	tx_helper::parachain::{fetch_genesis_header, fetch_validation_code},
	LocalFileSystem, Network, NetworkConfig, NetworkNode,
};

pub const PARACHAIN_VALIDATOR_METRIC: &str = "polkadot_node_is_parachain_validator";
pub const ACTIVE_VALIDATOR_METRIC: &str = "polkadot_node_is_active_validator";
pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const CUMULUS_IMAGE_ENV: &str = "CUMULUS_IMAGE";
pub const COL_IMAGE_ENV: &str = "COL_IMAGE";
pub const MALUS_IMAGE_ENV: &str = "MALUS_IMAGE";
pub const BLOCK_HEIGHT_FINALIZED_METRIC: &str = "substrate_block_height{status=\"finalized\"}";
pub const APPROVAL_CHECKING_FINALITY_LAG_METRIC: &str =
	"polkadot_parachain_approval_checking_finality_lag";
pub const APPROVAL_NO_SHOWS_TOTAL_METRIC: &str = "polkadot_parachain_approvals_no_shows_total";
pub const DATA_RECOVERY_FROM_SYSTEMATIC_CHUNKS_COMPLETE_PATTERN: &str =
	"*Data recovery from systematic chunks complete*";
pub const DATA_RECOVERY_FROM_SYSTEMATIC_CHUNKS_NOT_POSSIBLE_PATTERN: &str =
	"*Data recovery from systematic chunks is not possible*";
pub const DATA_RECOVERY_CHUNKS_PATTERN: &str = "*Data recovery from chunks complete*";
pub const AVAILABILITY_RECOVERY_RECOVERIES_FINISHED: &str =
	"polkadot_parachain_availability_recovery_recoveries_finished{result=\"failure\"}";
pub const NODE_ROLES_METRIC: &str = "node_roles";

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

pub async fn check_log_lines(
	validator_nodes: &[&NetworkNode],
	checks: &[(&str, LogLineCountOptions)],
) -> Result<(), anyhow::Error> {
	for (pattern, opts) in checks {
		for validator in validator_nodes {
			let result =
				validator.wait_log_line_count_with_timeout(*pattern, true, opts.clone()).await?;

			assert!(
				result.success(),
				"Can't find a matching line ({pattern}) in node {}",
				validator.name()
			);
		}
		log::info!("All nodes pass the log line match - {pattern}");
	}
	Ok(())
}

pub type MetricCheckSetup<'a> = (&'a str, Box<dyn Fn(f64) -> bool>, u64);
pub async fn check_metrics(
	validator_nodes: &[&NetworkNode],
	metric_checks: &[MetricCheckSetup<'_>],
) -> Result<(), anyhow::Error> {
	for (metric, predicate, timeout) in metric_checks {
		for validator in validator_nodes {
			let res = if *timeout == 0 {
				let res = validator.assert_with(*metric, &predicate).await?;
				if !res {
					Err(anyhow!("target value for metric {metric} doesn't pass the predicate"))
				} else {
					Ok(())
				}
			} else {
				validator.wait_metric_with_timeout(*metric, &predicate, *timeout).await
			};

			res.map_err(|e| anyhow!("node {} check failed ({metric}): {e}", validator.name()))?;
		}
		log::info!("All nodes pass the metric {metric} predicate");
	}

	Ok(())
}

/// Get genesis header and validation code for parachain
pub async fn fetch_header_and_validation_code(
	para_client: &OnlineClient<PolkadotConfig>,
) -> Result<(Vec<u8>, Vec<u8>), anyhow::Error> {
	log::info!("Fetching genesis header and validation code for parachain");
	let genesis_header = fetch_genesis_header(para_client).await?;
	let validation_code = fetch_validation_code(para_client).await?;

	log::info!(
		"Genesis header: {} bytes, Validation code: {} bytes",
		genesis_header.len(),
		validation_code.len()
	);

	Ok((genesis_header, validation_code))
}
/// Creates calls to force register the parachain and add valication code as trusted.
pub fn create_force_register_call(
	genesis_header: &[u8],
	validation_code: &[u8],
	para_id: u32,
	registrar_account: Value,
) -> Vec<Value> {
	let genesis_head_value = Value::from_bytes(genesis_header);
	let validation_code_value = Value::from_bytes(validation_code);

	let force_register_call = value! {
		Registrar(force_register { who: registrar_account, deposit: 0u128, id: para_id, genesis_head: genesis_head_value, validation_code: validation_code_value.clone() })
	};

	let add_trusted_validation_code_call = value! {
		Paras(add_trusted_validation_code { validation_code: validation_code_value })
	};

	let calls = vec![add_trusted_validation_code_call, force_register_call];

	calls
}

/// Check if all the nodes are validators (node_roles == 4.0)
pub async fn assert_nodes_are_validators(nodes: &[&NetworkNode]) -> Result<(), anyhow::Error> {
	for node in nodes {
		node.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 60u64)
			.await
			.map_err(|e| anyhow!("Validator {} role check failed: {e}", node.name()))?;
	}

	Ok(())
}
