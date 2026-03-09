// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::{
	any::Any,
	path::{Path, PathBuf},
	time::Duration,
};

use anyhow::anyhow;
use codec::Encode;
use futures::StreamExt;
use log::info;
use sp_core::{hexdisplay::HexDisplay, sr25519, Bytes, Pair};
use sp_statement_store::{
	statement_allowance_key, Channel, StatementAllowance, StatementEvent, SubmitResult, Topic,
	TopicFilter,
};
use zombienet_sdk::{
	subxt::{
		backend::rpc::RpcClient,
		config::{
			transaction_extensions::{
				AnyOf, ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis,
				CheckMetadataHash, CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion,
				TransactionExtension, VerifySignatureDetails,
			},
			Config, DefaultExtrinsicParamsBuilder, ExtrinsicParams, ExtrinsicParamsEncoder,
		},
		dynamic::Value,
		ext::{scale_value::value, subxt_rpcs::rpc_params},
		tx::{signer::Signer, DynamicPayload, TxStatus},
		utils::{Static, H256},
		OnlineClient, PolkadotConfig,
	},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

pub(super) const RPC_POOL_SIZE: usize = 10000;

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
		};
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

/// Creates a statement with multiple topics set
pub(super) fn create_multi_topic_statement(
	keypair: &sr25519::Pair,
	topics: &[Topic],
	data: Vec<u8>,
	expiry_ts: u32,
	seq: u32,
) -> sp_statement_store::Statement {
	let mut statement = sp_statement_store::Statement::new();
	for (i, topic) in topics.iter().enumerate() {
		statement.set_topic(i, *topic);
	}
	statement.set_plain_data(data);
	statement.set_expiry_from_parts(expiry_ts, seq);
	statement.sign_sr25519_private(keypair);
	statement
}

/// Creates a statement with a channel set
pub(super) fn create_channel_statement(
	keypair: &sr25519::Pair,
	topic: Topic,
	channel: Channel,
	data: Vec<u8>,
	expiry_ts: u32,
	seq: u32,
) -> sp_statement_store::Statement {
	let mut statement = sp_statement_store::Statement::new();
	statement.set_topic(0, topic);
	statement.set_channel(channel);
	statement.set_plain_data(data);
	statement.set_expiry_from_parts(expiry_ts, seq);
	statement.sign_sr25519_private(keypair);
	statement
}

/// Subscribes to statements matching any of the given topics
pub(super) async fn subscribe_topic_match_any(
	rpc: &RpcClient,
	topics: Vec<Topic>,
) -> Result<
	zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<StatementEvent>,
	anyhow::Error,
> {
	let subscription = rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAny(topics.try_into().expect("MatchAny topics"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	Ok(subscription)
}

/// Subscribes to all statements regardless of topic
pub(super) async fn subscribe_all(
	rpc: &RpcClient,
) -> Result<
	zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<StatementEvent>,
	anyhow::Error,
> {
	let subscription = rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::Any],
			"statement_unsubscribeStatement",
		)
		.await?;
	Ok(subscription)
}

/// Collects `count` statements from a subscription without assuming arrival order
///
/// Handles multi-item `NewStatements` batches by collecting all items from each batch
/// Returns the collected statements once the target count is reached
pub(super) async fn expect_statements_unordered(
	subscription: &mut zombienet_sdk::subxt::ext::subxt_rpcs::client::RpcSubscription<
		StatementEvent,
	>,
	count: usize,
	timeout_secs: u64,
) -> Result<Vec<Bytes>, anyhow::Error> {
	let mut collected = Vec::with_capacity(count);
	let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

	while collected.len() < count {
		let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
		if remaining.is_zero() {
			return Err(anyhow!(
				"Timeout after {}s: collected {}/{} statements",
				timeout_secs,
				collected.len(),
				count
			));
		}

		let item = tokio::time::timeout(remaining, subscription.next())
			.await
			.map_err(|_| {
				anyhow!(
					"Timeout after {}s: collected {}/{} statements",
					timeout_secs,
					collected.len(),
					count
				)
			})?
			.ok_or_else(|| anyhow!("Subscription stream ended unexpectedly"))?
			.map_err(|e| anyhow!("Subscription error: {}", e))?;

		match item {
			StatementEvent::NewStatements { statements: batch, .. } => {
				for stmt in batch {
					collected.push(stmt);
				}
			},
		}
	}

	Ok(collected)
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
	let max_subs_per_conn = (participant_count * 16 / RPC_POOL_SIZE as u32).max(32);

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

/// Creates storage items for custom per-participant allowances
pub(super) fn create_allowance_items(
	allowances: &[(u32, StatementAllowance)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut items = Vec::with_capacity(allowances.len());
	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance.encode()));
	}
	items
}

/// Creates uniform allowance storage items for a range of participants
pub(super) fn create_uniform_allowance_items(
	count: u32,
	allowance: StatementAllowance,
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let allowance_encoded = allowance.encode();
	let mut items = Vec::with_capacity(count as usize);
	for idx in 0..count {
		let keypair = get_keypair(idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance_encoded.clone()));
	}
	items
}

/// Creates a sudo -> frame_system::set_storage call to set statement allowances
pub(super) fn create_set_storage_call(items: Vec<(Vec<u8>, Vec<u8>)>) -> DynamicPayload {
	let items_value: Vec<Value> = items
		.into_iter()
		.map(|(key, value)| value!((Value::from_bytes(key), Value::from_bytes(value))))
		.collect();

	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items_value })
		}],
	)
}

/// Submits an extrinsic with an explicit nonce and waits for it to be included in a block
pub(super) async fn submit_sudo_extrinsic<S: Signer<BenchConfig>>(
	client: &OnlineClient<BenchConfig>,
	call: &DynamicPayload,
	signer: &S,
	nonce: u64,
) -> Result<
	zombienet_sdk::subxt::tx::TxProgress<BenchConfig, OnlineClient<BenchConfig>>,
	anyhow::Error,
> {
	let dp = DefaultExtrinsicParamsBuilder::<BenchConfig>::new()
		.immortal()
		.nonce(nonce)
		.build();
	let extensions =
		(dp.0, dp.1, dp.2, dp.3, dp.4, dp.5, dp.6, dp.7, dp.8, (), (), (), (), (), (), (), ());

	let mut tx = client
		.tx()
		.create_signed(call, signer, extensions)
		.await?
		.submit_and_watch()
		.await?;

	while let Some(status) = tx.next().await.transpose()? {
		match status {
			TxStatus::InBestBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::InFinalizedBlock(ref tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow!("Error submitting sudo tx: {message}"));
			},
			_ => continue,
		}
	}

	Err(anyhow!("Transaction event stream ended without being included in a block"))
}

/// Waits for a tx to finalize
pub(super) async fn wait_for_tx_finalization<Tx>(
	tx_stream: &mut Tx,
	timeout_secs: u64,
) -> Result<H256, anyhow::Error>
where
	Tx: futures::Stream<
			Item = Result<
				TxStatus<BenchConfig, OnlineClient<BenchConfig>>,
				zombienet_sdk::subxt::Error,
			>,
		> + Unpin,
{
	let watch_future = async {
		while let Some(status) = tx_stream.next().await.transpose()? {
			match status {
				TxStatus::InFinalizedBlock(ref tx_in_block) => {
					tx_in_block.wait_for_success().await?;
					return Ok(tx_in_block.block_hash());
				},
				TxStatus::Error { message } |
				TxStatus::Invalid { message } |
				TxStatus::Dropped { message } => {
					return Err(anyhow!("Tx error during finalization: {message}"));
				},
				_ => continue,
			}
		}
		Err(anyhow!("Transaction stream ended without finalization"))
	};

	tokio::time::timeout(Duration::from_secs(timeout_secs), watch_future)
		.await
		.map_err(|_| anyhow!("Timeout waiting for tx finalization after {}s", timeout_secs))?
}

/// Gets the current nonce for an account
pub(super) async fn get_account_nonce(
	client: &OnlineClient<BenchConfig>,
	account_id: &<BenchConfig as Config>::AccountId,
) -> Result<u64, anyhow::Error> {
	let nonce = client.tx().account_nonce(account_id).await?;
	Ok(nonce)
}

/// Sets statement allowances via sudo -> frame_system::set_storage extrinsic
pub(super) async fn set_allowances_via_sudo(
	para_client: &OnlineClient<BenchConfig>,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	info!("Setting {} statement allowances via sudo...", items.len());

	let alice = zombienet_sdk::subxt_signer::sr25519::dev::alice();
	let alice_account_id =
		<zombienet_sdk::subxt_signer::sr25519::Keypair as Signer<BenchConfig>>::account_id(&alice);

	let current_nonce = get_account_nonce(para_client, &alice_account_id).await?;
	let set_storage_call = create_set_storage_call(items);

	let mut tx_stream =
		submit_sudo_extrinsic(para_client, &set_storage_call, &alice, current_nonce).await?;
	let block_hash = wait_for_tx_finalization(&mut tx_stream, 120).await?;
	info!("Statement allowances set and finalized in block {:?}", block_hash);

	Ok(())
}

/// Spawns a network with the sudo-enabled chain spec and sets allowances at runtime
pub(super) async fn spawn_network_sudo(
	collators: &[&str],
	allowance_items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let participant_count = allowance_items.len();

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
				.with_id(1004)
				.with_chain_spec_path("https://raw.githubusercontent.com/paritytech/chainspecs/7c37889e49346b8fd44b20f9f83ce62fccbcbb11/versi/parachain/versi-people-1004/people-westend-spec.json")
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", participant_count + 1000).as_str().into(),
					format!(
						"--rpc-max-subscriptions-per-connection={}",
						(participant_count * 16).max(32)
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

	info!("Waiting for parachain to produce blocks...");
	let first_collator = collators[0];
	let node = network.get_node(first_collator)?;
	node.wait_metric_with_timeout("block_height{status=\"best\"}", |height| height >= 1.0, 120u64)
		.await?;
	info!("Parachain is producing blocks");

	let para_client = node.wait_client::<BenchConfig>().await?;
	set_allowances_via_sudo(&para_client, allowance_items).await?;

	Ok(network)
}

pub(super) struct VerifyMultiSignature<T: Config>(VerifySignatureDetails<T>);

impl<T: Config> ExtrinsicParams<T> for VerifyMultiSignature<T> {
	type Params = ();

	fn new(
		_client: &zombienet_sdk::subxt::client::ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, zombienet_sdk::subxt::config::ExtrinsicParamsError> {
		Ok(VerifyMultiSignature(VerifySignatureDetails::Disabled))
	}
}

impl<T: Config> ExtrinsicParamsEncoder for VerifyMultiSignature<T> {
	fn encode_value_to(&self, v: &mut Vec<u8>) {
		self.0.encode_to(v);
	}

	fn inject_signature(&mut self, account: &dyn Any, signature: &dyn Any) {
		let account = account
			.downcast_ref::<T::AccountId>()
			.expect("A T::AccountId should have been provided")
			.clone();
		let signature = signature
			.downcast_ref::<T::Signature>()
			.expect("A T::Signature should have been provided")
			.clone();
		self.0 = VerifySignatureDetails::Signed { signature, account };
	}
}

impl<T: Config> TransactionExtension<T> for VerifyMultiSignature<T> {
	type Decoded = Static<VerifySignatureDetails<T>>;

	fn matches(identifier: &str, _type_id: u32, _types: &::scale_info::PortableRegistry) -> bool {
		identifier == "VerifyMultiSignature" || identifier == "VerifySignature"
	}
}

/// Check whether a type requires 0 bytes to encode
///
/// Empty types are automatically skipped by `AnyOf`, so catch-all handlers must not claim them
fn is_type_empty(type_id: u32, types: &::scale_info::PortableRegistry) -> bool {
	use scale_info::TypeDef;
	let Some(ty) = types.resolve(type_id) else {
		return false;
	};
	match &ty.type_def {
		TypeDef::Composite(c) => c.fields.iter().all(|f| is_type_empty(f.ty.id, types)),
		TypeDef::Array(a) => a.len == 0 || is_type_empty(a.type_param.id, types),
		TypeDef::Tuple(t) => t.fields.iter().all(|f| is_type_empty(f.id, types)),
		_ => false,
	}
}

macro_rules! define_skip_unknown_extensions {
	($($name:ident),+ $(,)?) => { $(
		pub(super) struct $name;

		impl<T: Config> ExtrinsicParams<T> for $name {
			type Params = ();

			fn new(
				_client: &zombienet_sdk::subxt::client::ClientState<T>,
				_params: Self::Params,
			) -> Result<Self, zombienet_sdk::subxt::config::ExtrinsicParamsError> {
				Ok($name)
			}
		}

		impl ExtrinsicParamsEncoder for $name {
			fn encode_value_to(&self, v: &mut Vec<u8>) {
				v.push(0x00);
			}
		}

		impl<T: Config> TransactionExtension<T> for $name {
			type Decoded = Static<u8>;

			fn matches(
				_identifier: &str,
				type_id: u32,
				types: &::scale_info::PortableRegistry,
			) -> bool {
				!is_type_empty(type_id, types)
			}
		}
	)+ };
}

define_skip_unknown_extensions!(
	SkipUnknown1,
	SkipUnknown2,
	SkipUnknown3,
	SkipUnknown4,
	SkipUnknown5,
	SkipUnknown6,
	SkipUnknown7,
	SkipUnknown8,
);

pub(super) type BenchExtrinsicParams<T> = AnyOf<
	T,
	(
		VerifyMultiSignature<T>,
		CheckSpecVersion,
		CheckTxVersion,
		CheckNonce,
		CheckGenesis<T>,
		CheckMortality<T>,
		ChargeAssetTxPayment<T>,
		ChargeTransactionPayment,
		CheckMetadataHash,
		SkipUnknown1,
		SkipUnknown2,
		SkipUnknown3,
		SkipUnknown4,
		SkipUnknown5,
		SkipUnknown6,
		SkipUnknown7,
		SkipUnknown8,
	),
>;

/// Custom subxt [`Config`] identical to [`PolkadotConfig`] but using [`BenchExtrinsicParams`]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub(super) enum BenchConfig {}

impl Config for BenchConfig {
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = BenchExtrinsicParams<Self>;
	type AssetId = <PolkadotConfig as Config>::AssetId;
}
