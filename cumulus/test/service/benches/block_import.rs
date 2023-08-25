// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

use sc_client_api::UsageProvider;

use core::time::Duration;
use cumulus_primitives_core::ParaId;

use sc_block_builder::{BlockBuilderProvider, RecordProof};
use sp_api::{Core, ProvideRuntimeApi};
use sp_keyring::Sr25519Keyring::Alice;

use cumulus_test_service::bench_utils as utils;

fn benchmark_block_import(c: &mut Criterion) {
	sp_tracing::try_init_simple();

	let runtime = tokio::runtime::Runtime::new().expect("creating tokio runtime doesn't fail; qed");
	let para_id = ParaId::from(100);
	let tokio_handle = runtime.handle();

	// Create enough accounts to fill the block with transactions.
	// Each account should only be included in one transfer.
	let (src_accounts, dst_accounts, account_ids) = utils::create_benchmark_accounts();

	let alice = runtime.block_on(
		cumulus_test_service::TestNodeBuilder::new(para_id, tokio_handle.clone(), Alice)
			// Preload all accounts with funds for the transfers
			.endowed_accounts(account_ids)
			.build(),
	);

	let client = alice.client;

	let (max_transfer_count, extrinsics) =
		utils::create_benchmarking_transfer_extrinsics(&client, &src_accounts, &dst_accounts);

	let parent_hash = client.usage_info().chain.best_hash;
	let mut block_builder =
		client.new_block_at(parent_hash, Default::default(), RecordProof::No).unwrap();
	for extrinsic in extrinsics {
		block_builder.push(extrinsic).unwrap();
	}
	let benchmark_block = block_builder.build().unwrap();

	let mut group = c.benchmark_group("Block import");
	group.sample_size(20);
	group.measurement_time(Duration::from_secs(120));
	group.throughput(Throughput::Elements(max_transfer_count as u64));

	group.bench_function(format!("(transfers = {}) block import", max_transfer_count), |b| {
		b.iter_batched(
			|| benchmark_block.block.clone(),
			|block| {
				client.runtime_api().execute_block(parent_hash, block).unwrap();
			},
			BatchSize::SmallInput,
		)
	});
}

criterion_group!(benches, benchmark_block_import);
criterion_main!(benches);
