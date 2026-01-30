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
use sp_statement_store::Statement;
use std::sync::Arc;

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
	let topic = [0u8; 32];
	let topic1 = [1u8; 32];
	let topic2 = [2u8; 32];

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
	let api_rpc_clone = api_rpc.clone();
	let submitted = generate_statements();
	let first_topic: Bytes = submitted[0].topic(0).expect("Should have topic").to_vec().into();

	let match_all_filter =
		TopicFilter::MatchAll(vec![first_topic.clone()].try_into().expect("Single topic"));
	let submitted_clone = submitted.clone();
	let match_any_filter = TopicFilter::MatchAny(
		vec![
			submitted[0].topic(1).expect("Should have topic").to_vec().into(),
			submitted[1].topic(1).expect("Should have topic").to_vec().into(),
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
				let _: SubmitResult = api_rpc_clone
					.call("statement_submit", (encoded_statement,))
					.await
					.expect("Failed to submit statement");
			}
		}
		.boxed(),
	);

	for subscription in subscriptions.into_iter() {
		check_submitted(submitted.clone(), subscription).await;
	}

	// Check subscribing after initial statements gets all statements through as well.
	let subscriptions =
		subscribe_to_topics(&api_rpc, vec![match_all_filter, TopicFilter::Any, match_any_filter])
			.await;

	for subscription in subscriptions.into_iter() {
		check_submitted(submitted.clone(), subscription).await;
	}

	let mut match_any_with_random = api_rpc
		.subscribe_unbounded(
			"statement_subscribeStatement",
			(TopicFilter::MatchAny(vec![vec![7u8; 32].into()].try_into().expect("Single topic")),),
		)
		.await
		.expect("Failed to subscribe");

	let res = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		match_any_with_random.next::<Bytes>(),
	)
	.await;
	assert!(res.is_err(), "expected no message for random topic");

	let match_all_with_random = TopicFilter::MatchAll(
		vec![first_topic, vec![7u8; 32].into()].try_into().expect("Two topics"),
	);
	let mut match_all_with_random = api_rpc
		.subscribe("statement_subscribeStatement", (match_all_with_random,), 100000)
		.await
		.expect("Failed to subscribe");

	let res = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		match_all_with_random.next::<Bytes>(),
	)
	.await;
	assert!(res.is_err(), "expected no message for random topic");
}

async fn check_submitted(
	mut expected: Vec<sp_statement_store::Statement>,
	mut subscription: Subscription,
) {
	while !expected.is_empty() {
		let result = subscription.next::<Bytes>().await;
		let result = result.expect("Bytes").expect("Success").0;
		let new_statement =
			sp_statement_store::Statement::decode(&mut &result.0[..]).expect("Decode statement");
		let position = expected
			.iter()
			.position(|x| x == &new_statement)
			.expect("Statement should exist");
		expected.remove(position);
	}
}
