// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarks for preparation through the host. We use a real PVF to get realistic results.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, SamplingMode};
use polkadot_node_core_pvf::{
	start, testing, Config, Metrics, PrepareError, PrepareJobKind, PvfPrepData, ValidationHost,
};
use polkadot_primitives::ExecutorParams;
use rococo_runtime::WASM_BINARY;
use std::time::Duration;
use tokio::{runtime::Handle, sync::Mutex};

const TEST_PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);

struct TestHost {
	// Keep a reference to the tempdir otherwise it gets deleted on drop.
	#[allow(dead_code)]
	cache_dir: tempfile::TempDir,
	host: Mutex<ValidationHost>,
}

impl TestHost {
	async fn new_with_config<F>(handle: &Handle, f: F) -> Self
	where
		F: FnOnce(&mut Config),
	{
		let (prepare_worker_path, execute_worker_path) = testing::build_workers_and_get_paths();

		let cache_dir = tempfile::tempdir().unwrap();
		let mut config = Config::new(
			cache_dir.path().to_owned(),
			None,
			false,
			prepare_worker_path,
			execute_worker_path,
		);
		f(&mut config);
		let (host, task) = start(config, Metrics::default()).await.unwrap();
		let _ = handle.spawn(task);
		Self { host: Mutex::new(host), cache_dir }
	}

	async fn precheck_pvf(
		&self,
		code: &[u8],
		executor_params: ExecutorParams,
	) -> Result<(), PrepareError> {
		let (result_tx, result_rx) = futures::channel::oneshot::channel();

		let code = sp_maybe_compressed_blob::decompress(code, 16 * 1024 * 1024)
			.expect("Compression works");

		self.host
			.lock()
			.await
			.precheck_pvf(
				PvfPrepData::from_code(
					code.into(),
					executor_params,
					TEST_PREPARATION_TIMEOUT,
					PrepareJobKind::Prechecking,
				),
				result_tx,
			)
			.await
			.unwrap();
		result_rx.await.unwrap()
	}
}

fn host_prepare_rococo_runtime(c: &mut Criterion) {
	polkadot_node_core_pvf_common::sp_tracing::try_init_simple();

	let rt = tokio::runtime::Runtime::new().unwrap();

	let blob = WASM_BINARY.expect("You need to build the WASM binaries to run the tests!");
	let pvf = match sp_maybe_compressed_blob::decompress(&blob, 64 * 1024 * 1024) {
		Ok(code) => PvfPrepData::from_code(
			code.into_owned(),
			ExecutorParams::default(),
			Duration::from_secs(360),
			PrepareJobKind::Compilation,
		),
		Err(e) => {
			panic!("Cannot decompress blob: {:?}", e);
		},
	};

	let mut group = c.benchmark_group("prepare rococo");
	group.sampling_mode(SamplingMode::Flat);
	group.sample_size(20);
	group.measurement_time(Duration::from_secs(240));
	group.bench_function("host: prepare Rococo runtime", |b| {
		b.to_async(&rt).iter_batched(
			|| async {
				(
					TestHost::new_with_config(rt.handle(), |cfg| {
						cfg.prepare_workers_hard_max_num = 1;
					})
					.await,
					pvf.clone().code(),
				)
			},
			|result| async move {
				let (host, pvf_code) = result.await;

				// `PvfPrepData` is designed to be cheap to clone, so cloning shouldn't affect the
				// benchmark accuracy.
				let _stats = host.precheck_pvf(&pvf_code, Default::default()).await.unwrap();
			},
			BatchSize::SmallInput,
		)
	});
	group.finish();
}

criterion_group!(prepare, host_prepare_rococo_runtime);
criterion_main!(prepare);
