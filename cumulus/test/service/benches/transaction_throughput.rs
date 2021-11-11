// This file is part of Cumulus.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use cumulus_test_runtime::{AccountId, BalancesCall, SudoCall};
use futures::{future, join, StreamExt};
use polkadot_service::polkadot_runtime::constants::currency::DOLLARS;
use sc_transaction_pool_api::{TransactionPool as _, TransactionSource, TransactionStatus};
use sp_core::{crypto::Pair, sr25519};
use sp_runtime::{generic::BlockId, OpaqueExtrinsic};

use cumulus_primitives_core::ParaId;
use cumulus_test_service::{
	construct_extrinsic, fetch_nonce, initial_head_data, Client, Keyring::*, TransactionPool,
};

fn create_accounts(num: usize) -> Vec<sr25519::Pair> {
	(0..num)
		.map(|i| {
			Pair::from_string(&format!("{}/{}", Alice.to_seed(), i), None)
				.expect("Creates account pair")
		})
		.collect()
}

/// Create the extrinsics that will initialize the accounts from the sudo account (Alice).
///
/// `start_nonce` is the current nonce of Alice.
fn create_account_extrinsics(client: &Client, accounts: &[sr25519::Pair]) -> Vec<OpaqueExtrinsic> {
	let start_nonce = fetch_nonce(client, Alice.public());

	accounts
		.iter()
		.enumerate()
		.map(|(i, a)| {
			vec![
				// Reset the nonce by removing any funds
				construct_extrinsic(
					client,
					SudoCall::sudo {
						call: Box::new(
							BalancesCall::set_balance {
								who: AccountId::from(a.public()).into(),
								new_free: 0,
								new_reserved: 0,
							}
							.into(),
						),
					},
					Alice.pair(),
					Some(start_nonce + (i as u32) * 2),
				),
				// Give back funds
				construct_extrinsic(
					client,
					SudoCall::sudo {
						call: Box::new(
							BalancesCall::set_balance {
								who: AccountId::from(a.public()).into(),
								new_free: 1_000_000 * DOLLARS,
								new_reserved: 0,
							}
							.into(),
						),
					},
					Alice.pair(),
					Some(start_nonce + (i as u32) * 2 + 1),
				),
			]
		})
		.flatten()
		.map(OpaqueExtrinsic::from)
		.collect()
}

fn create_benchmark_extrinsics(
	client: &Client,
	accounts: &[sr25519::Pair],
	extrinsics_per_account: usize,
) -> Vec<OpaqueExtrinsic> {
	accounts
		.iter()
		.map(|account| {
			(0..extrinsics_per_account).map(move |nonce| {
				construct_extrinsic(
					client,
					BalancesCall::transfer { dest: Bob.to_account_id().into(), value: 1 * DOLLARS },
					account.clone(),
					Some(nonce as u32),
				)
			})
		})
		.flatten()
		.map(OpaqueExtrinsic::from)
		.collect()
}

async fn submit_tx_and_wait_for_inclusion(
	tx_pool: &TransactionPool,
	tx: OpaqueExtrinsic,
	client: &Client,
	wait_for_finalized: bool,
) {
	let best_hash = client.chain_info().best_hash;

	let mut watch = tx_pool
		.submit_and_watch(&BlockId::Hash(best_hash), TransactionSource::External, tx.clone())
		.await
		.expect("Submits tx to pool")
		.fuse();

	loop {
		match watch.select_next_some().await {
			TransactionStatus::Finalized(_) => break,
			TransactionStatus::InBlock(_) if !wait_for_finalized => break,
			_ => {},
		}
	}
}

fn transaction_throughput_benchmarks(c: &mut Criterion) {
	sp_tracing::try_init_simple();
	let mut builder = sc_cli::LoggerBuilder::new("");
	builder.with_colors(false);
	let _ = builder.init();

	let para_id = ParaId::from(100);
	let runtime = tokio::runtime::Runtime::new().expect("Creates tokio runtime");
	let tokio_handle = runtime.handle();

	// Start alice
	let alice = cumulus_test_service::run_relay_chain_validator_node(
		tokio_handle.clone(),
		Alice,
		|| {},
		vec![],
	);

	// Start bob
	let bob = cumulus_test_service::run_relay_chain_validator_node(
		tokio_handle.clone(),
		Bob,
		|| {},
		vec![alice.addr.clone()],
	);

	// Register parachain
	runtime
		.block_on(
			alice.register_parachain(
				para_id,
				cumulus_test_service::runtime::WASM_BINARY
					.expect("You need to build the WASM binary to run this test!")
					.to_vec(),
				initial_head_data(para_id),
			),
		)
		.unwrap();

	// Run charlie as parachain collator
	let charlie = runtime.block_on(
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build(),
	);

	// Run dave as parachain collator
	let dave = runtime.block_on(
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Dave)
			.enable_collator()
			.connect_to_parachain_node(&charlie)
			.connect_to_relay_chain_nodes(vec![&alice, &bob])
			.build(),
	);

	runtime.block_on(dave.wait_for_blocks(1));

	let mut group = c.benchmark_group("Transaction pool");
	let account_num = 10;
	let extrinsics_per_account = 20;
	group.sample_size(10);
	group.throughput(Throughput::Elements(account_num as u64 * extrinsics_per_account as u64));

	let accounts = create_accounts(account_num);
	let mut counter = 1;

	let benchmark_handle = tokio_handle.clone();
	group.bench_function(
		format!("{} transfers from {} accounts", account_num * extrinsics_per_account, account_num),
		|b| {
			b.iter_batched(
				|| {
					let prepare_extrinsics = create_account_extrinsics(&*dave.client, &accounts);

					benchmark_handle.block_on(future::join_all(
						prepare_extrinsics.into_iter().map(|tx| {
							submit_tx_and_wait_for_inclusion(
								&dave.transaction_pool,
								tx,
								&*dave.client,
								true,
							)
						}),
					));

					create_benchmark_extrinsics(&*dave.client, &accounts, extrinsics_per_account)
				},
				|extrinsics| {
					benchmark_handle.block_on(future::join_all(extrinsics.into_iter().map(|tx| {
						submit_tx_and_wait_for_inclusion(
							&dave.transaction_pool,
							tx,
							&*dave.client,
							false,
						)
					})));

					println!("Finished {}", counter);
					counter += 1;
				},
				BatchSize::SmallInput,
			)
		},
	);

	runtime.block_on(async {
		join!(
			alice.task_manager.clean_shutdown(),
			bob.task_manager.clean_shutdown(),
			charlie.task_manager.clean_shutdown(),
			dave.task_manager.clean_shutdown(),
		)
	});
}

criterion_group!(benches, transaction_throughput_benchmarks);
criterion_main!(benches);
