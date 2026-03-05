// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::{
	path::{Path, PathBuf},
	time::Duration,
};

use anyhow::anyhow;
use codec::Encode;
use log::info;
use sp_core::{Bytes, Pair, hexdisplay::HexDisplay, sr25519};
use sp_statement_store::{
	StatementAllowance, StatementEvent, SubmitResult, Topic, TopicFilter, statement_allowance_key,
};
use zombienet_sdk::{
	LocalFileSystem, Network, NetworkConfigBuilder,
	subxt::{backend::rpc::RpcClient, ext::subxt_rpcs::rpc_params},
};

pub(super) fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementBench//{idx}"), None).expect("Valid seed")
}

pub(super) fn create_test_statement(
	keypair: &sr25519::Pair,
	topic: Topic,
	data: Vec<u8>,
	expiry_ts: u32,
	seq: u32,
) -> sp_statement_store::Statement {
	let mut statement = sp_statement_store::Statement::new();
	statement.set_topic(0, topic);
	statement.set_plain_data(data);
	statement.set_expiry_from_parts(expiry_ts, seq);
	statement.sign_sr25519_private(keypair);
	statement
}

pub(super) async fn submit_statement(
	rpc: &RpcClient,
	statement: &sp_statement_store::Statement,
) -> Result<SubmitResult, anyhow::Error> {
	let encoded: Bytes = statement.encode().into();
	let result: SubmitResult = rpc.request("statement_submit", rpc_params![encoded]).await?;
	Ok(result)
}

pub(super) async fn expect_statement(
	subscription: &mut zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<
		StatementEvent,
	>,
	timeout_secs: u64,
) -> Result<Bytes, anyhow::Error> {
	loop {
		let item = tokio::time::timeout(Duration::from_secs(timeout_secs), subscription.next())
			.await
			.map_err(|_| anyhow!("Timeout waiting for statement after {}s", timeout_secs))?
			.ok_or_else(|| anyhow!("Subscription stream ended unexpectedly"))?
			.map_err(|e| anyhow!("Subscription error: {}", e))?;

		return match item {
			StatementEvent::NewStatements { statements: batch, .. } => {
				if batch.is_empty() {
					continue;
				}
				assert_eq!(batch.len(), 1, "Expected exactly one statement in batch");
				Ok(batch.into_iter().next().unwrap())
			},
		}
	}
}

pub(super) async fn assert_no_more_statements(
	subscription: &mut zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<
		StatementEvent,
	>,
	timeout_secs: u64,
) -> Result<(), anyhow::Error> {
	let result = tokio::time::timeout(Duration::from_secs(timeout_secs), subscription.next()).await;
	assert!(result.is_err(), "Expected no more statements but received one");
	Ok(())
}

/// Subscribes to statements matching a specific topic
pub(super) async fn subscribe_topic(
	rpc: &RpcClient,
	topic: Topic,
) -> Result<
	zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<StatementEvent>,
	anyhow::Error,
> {
	let subscription = rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	Ok(subscription)
}

/// Creates a custom chain spec with uniform allowances for all participants.
///
/// Returns the path to the temporary chain spec file.
///
/// The chain spec template generates by:
/// `polkadot-parachain build-spec --chain people-westend-local --raw`
fn create_chain_spec_with_allowances(
	participant_count: u32,
	base_dir: &Path,
) -> Result<PathBuf, anyhow::Error> {
	let chain_spec_template = include_str!("../people-westend-local-spec.json");
	let mut chain_spec: serde_json::Value = serde_json::from_str(chain_spec_template)
		.map_err(|e| anyhow!("Failed to parse chain spec JSON: {}", e))?;
	let genesis = chain_spec
		.get_mut("genesis")
		.and_then(|g| g.get_mut("raw"))
		.and_then(|r| r.get_mut("top"))
		.and_then(|t| t.as_object_mut())
		.ok_or_else(|| anyhow!("Failed to access genesis.raw.top in chain spec"))?;

	let allowance = StatementAllowance { max_count: 100_000, max_size: 1_000_000 };
	let allowance_hex = format!("0x{}", HexDisplay::from(&allowance.encode()));
	info!("Injecting statement allowance: {:}", allowance_hex);
	for idx in 0..participant_count {
		let keypair = get_keypair(idx);
		let account_id = keypair.public();

		let storage_key = statement_allowance_key(account_id.0);
		let storage_key_hex = format!("0x{}", HexDisplay::from(&storage_key));

		genesis.insert(storage_key_hex, serde_json::Value::String(allowance_hex.clone()));
	}

	let chain_spec_path = base_dir.join("people-westend-custom.json");
	let chain_spec_json = serde_json::to_string_pretty(&chain_spec)
		.map_err(|e| anyhow!("Failed to serialize chain spec: {}", e))?;

	std::fs::write(&chain_spec_path, chain_spec_json)
		.map_err(|e| anyhow!("Failed to write chain spec to file: {}", e))?;

	info!("Created custom chain spec at: {}", chain_spec_path.display());

	Ok(chain_spec_path)
}

/// Creates a custom chain spec with per-participant allowances.
///
/// Each entry in `allowances` maps a participant index to a custom `StatementAllowance`.
/// Participants not listed receive no allowance entry in the chain spec.
fn create_chain_spec_with_custom_allowances(
	allowances: &[(u32, StatementAllowance)],
	base_dir: &Path,
) -> Result<PathBuf, anyhow::Error> {
	let chain_spec_template = include_str!("../people-westend-local-spec.json");
	let mut chain_spec: serde_json::Value = serde_json::from_str(chain_spec_template)
		.map_err(|e| anyhow!("Failed to parse chain spec JSON: {}", e))?;
	let genesis = chain_spec
		.get_mut("genesis")
		.and_then(|g| g.get_mut("raw"))
		.and_then(|r| r.get_mut("top"))
		.and_then(|t| t.as_object_mut())
		.ok_or_else(|| anyhow!("Failed to access genesis.raw.top in chain spec"))?;

	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();

		let storage_key = statement_allowance_key(account_id.0);
		let storage_key_hex = format!("0x{}", HexDisplay::from(&storage_key));
		let allowance_hex = format!("0x{}", HexDisplay::from(&allowance.encode()));

		info!("Injecting allowance for participant {}: {}", idx, allowance_hex);
		genesis.insert(storage_key_hex, serde_json::Value::String(allowance_hex));
	}

	let chain_spec_path = base_dir.join("people-westend-custom.json");
	let chain_spec_json = serde_json::to_string_pretty(&chain_spec)
		.map_err(|e| anyhow!("Failed to serialize chain spec: {}", e))?;

	std::fs::write(&chain_spec_path, chain_spec_json)
		.map_err(|e| anyhow!("Failed to write chain spec to file: {}", e))?;

	info!("Created custom chain spec at: {}", chain_spec_path.display());

	Ok(chain_spec_path)
}

/// Spawns a network using a custom chain spec with injected statement allowances.
pub(super) async fn spawn_network(
	collators: &[&str],
	participant_count: u32,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	assert!(collators.len() >= 2);
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let chain_spec_path = create_chain_spec_with_allowances(participant_count, &base_dir)?;
	// Headroom for the ~5,000 subscriptions that
	// actually end up on each pooled conn (500 participants * 10 subscriptions each).
	let max_subs_per_conn = participant_count / 10000 as u32 * 16;

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			let p = p
				.with_id(2400)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", participant_count + 1000).as_str().into(),
					format!("--rpc-max-subscriptions-per-connection={max_subs_per_conn}")
						.as_str()
						.into(),
				])
				// Have to set outside of the loop below, so that `p` has the right type.
				.with_collator(|n| n.with_name(collators[0]));

			collators[1..]
				.iter()
				.fold(p, |acc, &name| acc.with_collator(|n| n.with_name(name)))
		})
		.with_global_settings(|global_settings| {
			global_settings.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	Ok(network)
}

/// Spawns a network using a custom chain spec with per-participant allowances.
///
/// Unlike `spawn_network` which gives all participants the same high allowance,
/// this function accepts a list of `(participant_idx, allowance)` pairs allowing
/// fine-grained control over each participant's statement limits
pub(super) async fn spawn_network_with_custom_allowances(
	collators: &[&str],
	allowances: &[(u32, StatementAllowance)],
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	assert!(collators.len() >= 2);
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let chain_spec_path = create_chain_spec_with_custom_allowances(allowances, &base_dir)?;

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			let p = p
				.with_id(2400)
				.with_chain_spec_path(chain_spec_path.to_str().expect("Valid UTF-8 path"))
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", allowances.len() + 100).as_str().into(),
					format!(
						"--rpc-max-subscriptions-per-connection={}",
						allowances.len() * 16 + 16
					)
					.as_str()
					.into(),
				])
				.with_collator(|n| n.with_name(collators[0]));

			collators[1..]
				.iter()
				.fold(p, |acc, &name| acc.with_collator(|n| n.with_name(name)))
		})
		.with_global_settings(|global_settings| {
			global_settings.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	Ok(network)
}
