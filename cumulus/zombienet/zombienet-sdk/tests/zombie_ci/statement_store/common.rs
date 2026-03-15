// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::{
	path::{Path, PathBuf},
	time::Duration,
};

use anyhow::anyhow;
use codec::Encode;
use log::info;
use sp_core::{hexdisplay::HexDisplay, sr25519, Bytes, Pair};
use sp_statement_store::{
	statement_allowance_key, Channel, StatementAllowance, StatementEvent, SubmitResult, Topic,
	TopicFilter,
};
use zombienet_sdk::subxt::{
	backend::rpc::RpcClient,
	ext::subxt_rpcs::{client::RpcSubscription, rpc_params},
};

pub(super) fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementStoreClient//{idx}"), None).expect("Valid seed")
}

pub(super) fn create_test_statement(
	keypair: &sr25519::Pair,
	topics: &[Topic],
	channel: Option<Channel>,
	data: Vec<u8>,
	expiry_ts: u32,
	seq: u32,
) -> sp_statement_store::Statement {
	let mut statement = sp_statement_store::Statement::new();
	for (i, topic) in topics.iter().enumerate() {
		statement.set_topic(i, *topic);
	}
	if let Some(ch) = channel {
		statement.set_channel(ch);
	}
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

pub(super) async fn expect_one_statement(
	subscription: &mut RpcSubscription<StatementEvent>,
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
	subscription: &mut RpcSubscription<StatementEvent>,
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
) -> Result<RpcSubscription<StatementEvent>, anyhow::Error> {
	let subscription = rpc
		.subscribe::<StatementEvent>(
			"statement_subscribeStatement",
			rpc_params![TopicFilter::MatchAll(vec![topic].try_into().expect("Single topic"))],
			"statement_unsubscribeStatement",
		)
		.await?;
	Ok(subscription)
}

/// Subscribes to statements matching any of the given topics
pub(super) async fn subscribe_topic_match_any(
	rpc: &RpcClient,
	topics: Vec<Topic>,
) -> Result<RpcSubscription<StatementEvent>, anyhow::Error> {
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
) -> Result<RpcSubscription<StatementEvent>, anyhow::Error> {
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
	subscription: &mut RpcSubscription<StatementEvent>,
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
pub(super) fn create_chain_spec_with_allowances(
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
