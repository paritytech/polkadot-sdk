// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::submit_extrinsic_and_wait_for_finalization_success_with_timeout;
use zombienet_sdk::{
	subxt::{
		dynamic::Value, ext::scale_value::value, tx::DynamicPayload, OnlineClient, PolkadotConfig,
	},
	subxt_signer::sr25519::dev,
	LocalFileSystem, Network, NetworkConfig,
};

pub const PARACHAIN_VALIDATOR_METRIC: &str = "polkadot_node_is_parachain_validator";
pub const ACTIVE_VALIDATOR_METRIC: &str = "polkadot_node_is_active_validator";
pub const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
pub const CUMULUS_IMAGE_ENV: &str = "CUMULUS_IMAGE";
pub const MALUS_IMAGE_ENV: &str = "MALUS_IMAGE";
pub const COL_IMAGE_ENV: &str = "COL_IMAGE";
pub const NODE_ROLES_METRIC: &str = "node_roles";
pub const PEERS_COUNT_METRIC: &str = "substrate_sub_libp2p_peers_count";
pub const IS_MAJOR_SYNCING_METRIC: &str = "substrate_sub_libp2p_is_major_syncing";
pub const DISPUTES_TOTAL_METRIC: &str = "polkadot_parachain_candidate_disputes_total";
pub const DISPUTE_VOTES_VALID_METRIC: &str =
	"polkadot_parachain_candidate_dispute_votes{validity=\"valid\"}";
pub const DISPUTE_CONCLUDED_VALID_METRIC: &str =
	"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}";
pub const DISPUTE_CONCLUDED_INVALID_METRIC: &str =
	"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";
pub const SUBSTRATE_BLOCK_HEIGHT_METRIC: &str = "substrate_block_height{status=\"best\"}";
pub const SUBSTRATE_BLOCK_HEIGHT_FINALIZED_METRIC: &str =
	"substrate_block_height{status=\"finalized\"}";
pub const APPROVAL_CHECKING_FINALITY_LAG_METRIC: &str =
	"polkadot_parachain_approval_checking_finality_lag";
pub const APPROVALS_NO_SHOWS_TOTAL_METRIC: &str = "polkadot_parachain_approvals_no_shows_total";
pub const AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC: &str =
	"polkadot_parachain_availability_recovery_recoveries_finished{result=\"failure\"}";
pub const FETCHED_SUCCESSFUL_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"succeeded\"}";
pub const FETCHED_FAILED_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"failed\"}";
pub const FETCHED_NOT_FOUND_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"not-found\"}";
pub const BLOCK_HEIGHT_METRIC: &str = "block_height{status=\"best\"}";

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

/// Registers the given parachains by fetching their genesis header and validation code
/// from the collator nodes, then submitting a sudo batch call.
///
/// Assumes collator nodes are named `collator-{para_id}`.
pub async fn register_paras<S: zombienet_sdk::subxt::tx::signer::Signer<PolkadotConfig>>(
	network: &Network<LocalFileSystem>,
	relay_client: &OnlineClient<PolkadotConfig>,
	signer: &S,
	para_ids: &[u32],
) -> Result<(), anyhow::Error> {
	let mut calls = vec![];
	let alice_account = Value::from_bytes(dev::alice().public_key().0);

	for para_id in para_ids {
		let collator_name = format!("collator-{para_id}");
		let collator_node = network.get_node(&collator_name)?;
		let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

		let genesis_header = fetch_genesis_header(&collator_client).await?;
		let validation_code = fetch_validation_code(&collator_client).await?;

		log::info!(
			"Para {}: genesis header {} bytes, validation code {} bytes",
			para_id,
			genesis_header.len(),
			validation_code.len()
		);

		let validation_code_value = Value::from_bytes(&validation_code);
		calls.push(value! {
			Paras(add_trusted_validation_code { validation_code: validation_code_value })
		});

		let genesis_head_value = Value::from_bytes(&genesis_header);
		let validation_code_for_register = Value::from_bytes(&validation_code);
		calls.push(value! {
			Registrar(force_register {
				who: alice_account.clone(),
				deposit: 0u128,
				id: *para_id,
				genesis_head: genesis_head_value,
				validation_code: validation_code_for_register
			})
		});
	}

	let sudo_batch = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: calls })
		}],
	);

	submit_extrinsic_and_wait_for_finalization_success_with_timeout(
		relay_client,
		&sudo_batch,
		signer,
		600u64,
	)
	.await
	.map_err(|e| anyhow!("Failed to register paras {:?}: {}", para_ids, e))?;

	log::info!("Parachains {:?} registered successfully", para_ids);
	Ok(())
}
