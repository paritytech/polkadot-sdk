// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::testing::test_executor;
use codec::Encode;
use futures::FutureExt;
use jsonrpsee::{RpcModule, Subscription};
use sc_statement_store::Store;
use sp_core::traits::SpawnNamed;
use sp_statement_store::{statement_allowance_key, Statement, StatementAllowance, Topic};
use std::sync::Arc;
use substrate_test_runtime_client::{TestClientBuilder, TestClientBuilderExt};

async fn subscribe_to_topics(
	api_rpc: &RpcModule<StatementStore>,
	topic_filters: Vec<TopicFilter>,
) -> Vec<Subscription> {
	let mut subscriptions = Vec::with_capacity(topic_filters.len());
	for filter in topic_filters {
		let subscription = api_rpc
			.subscribe_unbounded("statement_subscribeStatement", (filter,))
			.await
			.expect("Failed to subscribe");
		subscriptions.push(subscription);
	}
	subscriptions
}

fn generate_statements() -> Vec<Statement> {
	let topic: Topic = [0u8; 32].into();
	let topic1: Topic = [1u8; 32].into();
	let topic2: Topic = [2u8; 32].into();

	let mut statements = Vec::new();
	let mut statement = sp_statement_store::Statement::new();
	statement.set_topic(0, topic);
	statement.set_topic(1, topic2);

	statement
		.set_proof(sp_statement_store::Proof::Ed25519 { signature: [0u8; 64], signer: [0u8; 32] });
	statement.set_expiry_from_parts(u32::MAX, 1);

	statements.push(statement.clone());

	let mut statement = sp_statement_store::Statement::new();
	statement.set_topic(0, topic);
	statement.set_topic(1, topic1);
	statement
		.set_proof(sp_statement_store::Proof::Ed25519 { signature: [0u8; 64], signer: [0u8; 32] });
	statement.set_expiry_from_parts(u32::MAX, 1);

	statements.push(statement.clone());
	statements
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn subscribe_works() {
	let executor = test_executor();
	let client = Arc::new(
		TestClientBuilder::with_default_backend()
			.add_extra_storage(
				statement_allowance_key([0; 32]),
				StatementAllowance { max_count: 1000, max_size: 10_000_000 }.encode(),
			)
			.build(),
	);
	let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
	let store = Store::new_shared(
		temp_dir.path(),
		Default::default(),
		Arc::clone(&client) as Arc<_>,
		Arc::new(sc_keystore::LocalKeystore::in_memory()),
		None,
		Box::new(executor.as_ref().clone()),
	)
	.expect("Failed to create statement store");

	let api = super::StatementStore::new(Arc::clone(&store) as Arc<_>, executor.clone());
	let api_rpc = api.into_rpc();
	let api_rpc_clone = api_rpc.clone();
	let submitted = generate_statements();
	let first_topic = submitted[0].topic(0).expect("Should have topic");

	let match_all_filter =
		TopicFilter::MatchAll(vec![first_topic].try_into().expect("Single topic"));
	let submitted_clone = submitted.clone();
	let match_any_filter = TopicFilter::MatchAny(
		vec![
			submitted[0].topic(1).expect("Should have topic"),
			submitted[1].topic(1).expect("Should have topic"),
		]
		.try_into()
		.expect("Two topics"),
	);

	let subscriptions = subscribe_to_topics(
		&api_rpc,
		vec![match_all_filter.clone(), TopicFilter::Any, match_any_filter.clone()],
	)
	.await;

	executor.spawn(
		"test",
		None,
		async move {
			for statement in submitted_clone {
				let encoded_statement: Bytes = statement.encode().into();
				let result: SubmitResult = api_rpc_clone
					.call("statement_submit", (encoded_statement,))
					.await
					.expect("Failed to submit statement");
				assert_eq!(result, SubmitResult::New);
			}
		}
		.boxed(),
	);

	for subscription in subscriptions.into_iter() {
		check_submitted(submitted.clone(), None, subscription).await;
	}

	// Check subscribing after initial statements gets all statements through as well.
	let subscriptions =
		subscribe_to_topics(&api_rpc, vec![match_all_filter, TopicFilter::Any, match_any_filter])
			.await;

	for subscription in subscriptions.into_iter() {
		check_submitted(submitted.clone(), Some(submitted.len() as u32), subscription).await;
	}

	let mut match_any_with_random = api_rpc
		.subscribe_unbounded(
			"statement_subscribeStatement",
			(TopicFilter::MatchAny(vec![Topic::from([7u8; 32])].try_into().expect("Single topic")),),
		)
		.await
		.expect("Failed to subscribe");

	// An empty NewStatements is sent when no existing statements match the filter.
	let result = match_any_with_random.next::<StatementEvent>().await;
	let StatementEvent::NewStatements { statements: batch, .. } =
		result.expect("Bytes").expect("Success").0;
	assert!(batch.is_empty(), "Expected empty batch for random topic, got: {:?}", batch);

	let res = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		match_any_with_random.next::<StatementEvent>(),
	)
	.await;
	assert!(res.is_err(), "expected no more messages for random topic");

	let match_all_with_random = TopicFilter::MatchAll(
		vec![first_topic, Topic::from([7u8; 32])].try_into().expect("Two topics"),
	);
	let mut match_all_with_random = api_rpc
		.subscribe("statement_subscribeStatement", (match_all_with_random,), 100000)
		.await
		.expect("Failed to subscribe");

	// An empty NewStatements is sent when no existing statements match the filter.
	let result = match_all_with_random.next::<StatementEvent>().await;
	let StatementEvent::NewStatements { statements: batch, .. } =
		result.expect("Bytes").expect("Success").0;
	assert!(batch.is_empty(), "Expected empty batch for random topic, got: {:?}", batch);

	let res = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		match_all_with_random.next::<StatementEvent>(),
	)
	.await;
	assert!(res.is_err(), "expected no more messages for random topic");
}

async fn check_submitted(
	mut expected: Vec<sp_statement_store::Statement>,
	_num_existing: Option<u32>,
	mut subscription: Subscription,
) {
	while !expected.is_empty() {
		let StatementEvent::NewStatements { statements: result, .. } =
			subscription.next::<StatementEvent>().await.expect("Bytes").expect("Success").0;
		if let Some(num_existing) = _num_existing {
			assert_eq!(
				result.len() as u32,
				num_existing,
				"Expected NumMatchingStatements with count of existing statements"
			);
		}
		for result in result {
			let new_statement = sp_statement_store::Statement::decode(&mut &result.0[..])
				.expect("Decode statement");
			let position = expected
				.iter()
				.position(|x| x == &new_statement)
				.expect("Statement should exist");
			expected.remove(position);
		}
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn subscribe_works_with_raw_json() {
	let executor = test_executor();
	let client = Arc::new(substrate_test_runtime_client::new());
	let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
	let store = Store::new_shared(
		temp_dir.path(),
		Default::default(),
		Arc::clone(&client) as Arc<_>,
		Arc::new(sc_keystore::LocalKeystore::in_memory()),
		None,
		Box::new(executor.as_ref().clone()),
	)
	.expect("Failed to create statement store");

	let api = super::StatementStore::new(Arc::clone(&store) as Arc<_>, executor.clone());
	let api_rpc = api.into_rpc();

	// Test subscription with raw JSON using "matchAll" filter with 4 topics
	let request = r#"{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":[{"matchAll":["0xdededededededededededededededededededededededededededededededede","0xadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadad","0xbebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebe","0xcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf"]}],"id":2}"#;
	let (response, _) = api_rpc
		.raw_json_request(request, 1)
		.await
		.expect("Raw JSON request should succeed");
	println!("response: {}", response);
	assert!(
		response.contains("\"result\""),
		"Expected successful subscription response, got: {}",
		response
	);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn subscribe_rejects_more_than_4_topics_in_match_all() {
	let executor = test_executor();
	let client = Arc::new(substrate_test_runtime_client::new());
	let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
	let store = Store::new_shared(
		temp_dir.path(),
		Default::default(),
		Arc::clone(&client) as Arc<_>,
		Arc::new(sc_keystore::LocalKeystore::in_memory()),
		None,
		Box::new(executor.as_ref().clone()),
	)
	.expect("Failed to create statement store");

	let api = super::StatementStore::new(Arc::clone(&store) as Arc<_>, executor.clone());
	let api_rpc = api.into_rpc();

	// Test subscription with raw JSON using "matchAll" filter with 5 topics (should be rejected)
	let request = r#"{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":[{"matchAll":["0xdededededededededededededededededededededededededededededededede","0xadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadadad","0xbebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebe","0xcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf","0x1111111111111111111111111111111111111111111111111111111111111111"]}],"id":2}"#;
	let (response, _) = api_rpc
		.raw_json_request(request, 1)
		.await
		.expect("Raw JSON request should succeed");
	println!("response: {}", response);
	assert!(
		response.contains("\"error\""),
		"Expected error response for more than 4 topics, got: {}",
		response
	);
}

#[tokio::test]
async fn send_in_chunks_empty_input() {
	let (sender, receiver) = async_channel::bounded(16);
	send_in_chunks(vec![], sender).await;
	assert!(receiver.try_recv().is_err(), "No messages should be sent for empty input");
}

#[tokio::test]
async fn send_in_chunks_single_small_statement() {
	let (sender, receiver) = async_channel::bounded(16);
	let statement = vec![1u8, 2, 3];
	send_in_chunks(vec![statement.clone()], sender).await;

	let StatementEvent::NewStatements { statements: bytes, .. } =
		receiver.try_recv().expect("Should receive one chunk");
	assert_eq!(bytes.len(), 1);
	assert_eq!(bytes[0].0, statement);
	assert!(receiver.try_recv().is_err(), "No more messages expected");
}

#[tokio::test]
async fn send_in_chunks_multiple_small_statements_fit_in_one_chunk() {
	let (sender, receiver) = async_channel::bounded(16);
	let statements: Vec<Vec<u8>> = (0..10).map(|i| vec![i; 100]).collect();
	send_in_chunks(statements.clone(), sender).await;

	let StatementEvent::NewStatements { statements: bytes, .. } =
		receiver.try_recv().expect("Should receive one chunk");
	assert_eq!(bytes.len(), 10);
	for (i, b) in bytes.iter().enumerate() {
		assert_eq!(b.0, statements[i]);
	}
	assert!(receiver.try_recv().is_err(), "No more messages expected");
}

#[tokio::test]
async fn send_in_chunks_splits_large_statements() {
	let (sender, receiver) = async_channel::bounded(16);
	// Each statement is 1 MB of SCALE bytes → 2 MB estimated JSON size.
	// MAX_CHUNK_BYTES_LIMIT is 4 MB, so at most 2 statements per chunk.
	let statement_size = 1024 * 1024;
	let statements: Vec<Vec<u8>> = (0u8..5).map(|i| vec![i; statement_size]).collect();
	send_in_chunks(statements.clone(), sender).await;

	let mut all_received = Vec::new();
	while let Ok(StatementEvent::NewStatements { statements: bytes, .. }) = receiver.try_recv() {
		// Each chunk's total JSON estimate must not exceed MAX_CHUNK_BYTES_LIMIT
		let json_size: usize = bytes.iter().map(|b| b.0.len() * 2).sum();
		assert!(
			json_size <= MAX_CHUNK_BYTES_LIMIT,
			"Chunk JSON size {} exceeds limit {}",
			json_size,
			MAX_CHUNK_BYTES_LIMIT
		);
		all_received.extend(bytes);
	}
	assert_eq!(all_received.len(), 5);
	for (i, b) in all_received.iter().enumerate() {
		assert_eq!(b.0, statements[i]);
	}
}

#[tokio::test]
async fn send_in_chunks_oversized_statement_is_skipped() {
	let (sender, receiver) = async_channel::bounded(16);
	// A single statement whose JSON estimate exceeds MAX_CHUNK_BYTES_LIMIT.
	// The function skips it because the chunk is empty and the statement alone exceeds the limit.
	let oversized = vec![0u8; MAX_CHUNK_BYTES_LIMIT];
	send_in_chunks(vec![oversized], sender).await;

	assert!(receiver.try_recv().is_err(), "Oversized statement should be silently dropped");
}

#[tokio::test]
async fn send_in_chunks_oversized_statement_between_normal_ones() {
	let (sender, receiver) = async_channel::bounded(16);
	let small1 = vec![1u8; 100];
	// JSON estimate = MAX_CHUNK_BYTES_LIMIT * 2, exceeds limit
	let oversized = vec![0u8; MAX_CHUNK_BYTES_LIMIT];
	let small2 = vec![2u8; 100];
	send_in_chunks(vec![small1.clone(), oversized, small2.clone()], sender).await;

	// The oversized statement is skipped; small1 and small2 are sent together in one chunk.
	let StatementEvent::NewStatements { statements: bytes, .. } =
		receiver.try_recv().expect("Should receive chunk with both small statements");
	assert_eq!(bytes.len(), 2);
	assert_eq!(bytes[0].0, small1);
	assert_eq!(bytes[1].0, small2);
	assert!(receiver.try_recv().is_err(), "No more messages expected");
}

#[tokio::test]
async fn send_in_chunks_boundary_exact_fit() {
	let (sender, receiver) = async_channel::bounded(16);
	// Create statements that exactly fill the limit: each is MAX_CHUNK_BYTES_LIMIT / 2 SCALE
	// bytes → MAX_CHUNK_BYTES_LIMIT JSON bytes estimate per statement.
	let half_limit = MAX_CHUNK_BYTES_LIMIT / 2;
	let s1 = vec![1u8; half_limit];
	let s2 = vec![2u8; half_limit];
	let s3 = vec![3u8; half_limit];
	send_in_chunks(vec![s1.clone(), s2.clone(), s3.clone()], sender).await;

	let mut chunks = Vec::new();
	while let Ok(StatementEvent::NewStatements { statements: bytes, .. }) = receiver.try_recv() {
		chunks.push(bytes);
	}

	assert_eq!(chunks.len(), 3, "Each statement should be its own chunk");
	assert_eq!(chunks[0].len(), 1);
	assert_eq!(chunks[0][0].0, s1);
	assert_eq!(chunks[1][0].0, s2);
	assert_eq!(chunks[2][0].0, s3);
}
