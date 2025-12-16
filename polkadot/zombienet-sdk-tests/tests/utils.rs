// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use zombienet_sdk::{
	subxt::{
		dynamic::Value, ext::scale_value::value, tx::DynamicPayload, OnlineClient, PolkadotConfig,
	},
	LocalFileSystem, Network, NetworkConfig,
};

pub const PARACHAIN_VALIDATOR_METRIC: &str = "polkadot_node_is_parachain_validator";
pub const ACTIVE_VALIDATOR_METRIC: &str = "polkadot_node_is_active_validator";
pub const COL_IMAGE_ENV: &str = "COL_IMAGE";
pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const CUMULUS_IMAGE_ENV: &str = "CUMULUS_IMAGE";
pub const MALUS_IMAGE_ENV: &str = "MALUS_IMAGE";

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

/// Fetches the genesis header from a parachain node
pub async fn fetch_genesis_header(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<Vec<u8>, anyhow::Error> {
	use zombienet_sdk::subxt::ext::codec::Encode;
	let genesis_hash = client.genesis_hash();
	let header = client
		.backend()
		.block_header(genesis_hash)
		.await?
		.ok_or_else(|| anyhow!("Failed to fetch genesis header"))?;
	Ok(header.encode())
}

/// Fetches the validation code from a parachain node
pub async fn fetch_validation_code(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<Vec<u8>, anyhow::Error> {
	let code_key = sp_core::storage::well_known_keys::CODE;
	client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(code_key)
		.await?
		.ok_or_else(|| anyhow!("Failed to fetch validation code"))
}

/// Creates a sudo call to deregister a validator
pub fn create_deregister_validator_call(stash_account: Value) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			ValidatorManager(deregister_validators { validators: (stash_account) })
		}],
	)
}

/// Creates a sudo call to register a validator
pub fn create_register_validator_call(stash_account: Value) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			ValidatorManager(register_validators { validators: (stash_account) })
		}],
	)
}

/// Creates a sudo batch call to register a parachain with trusted validation code
#[allow(dead_code)]
pub fn create_register_para_call(
	genesis_header: Vec<u8>,
	validation_code: Vec<u8>,
	para_id: u32,
	registrar_account: Value,
) -> DynamicPayload {
	let genesis_head_value = Value::from_bytes(&genesis_header);
	let validation_code_value = Value::from_bytes(&validation_code);
	let validation_code_for_trusted = Value::from_bytes(&validation_code);

	let add_trusted_code_call = value! {
		Paras(add_trusted_validation_code { validation_code: validation_code_for_trusted })
	};

	let force_register_call = value! {
		Registrar(force_register { who: registrar_account, deposit: 0u128, id: para_id, genesis_head: genesis_head_value, validation_code: validation_code_value })
	};

	let calls = vec![add_trusted_code_call, force_register_call];

	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: calls })
		}],
	)
}
