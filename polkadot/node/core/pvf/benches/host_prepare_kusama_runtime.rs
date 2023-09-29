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

#[cfg(feature = "ci-only-tests")]
use assert_matches::assert_matches;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use polkadot_node_core_pvf::{
	start, testing, Config, Metrics, PrepareError, PrepareJobKind, PrepareStats, PvfPrepData,
	ValidationHost,
};
use polkadot_primitives::ExecutorParams;
use std::{path::PathBuf, time::Duration};
use tokio::{runtime::Handle, sync::Mutex};

const TEST_PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);

struct TestHost {
	cache_dir: PathBuf,
	host: Mutex<ValidationHost>,
}

impl TestHost {
	fn new_with_config<F>(handle: &Handle, f: F) -> Self
	where
		F: FnOnce(&mut Config),
	{
		let (prepare_worker_path, execute_worker_path) = testing::get_and_check_worker_paths();

		let cache_dir = std::path::Path::new("/tmp/test").to_owned();
		// let cache_dir = tempfile::tempdir().unwrap();
		let mut config =
			Config::new(cache_dir.clone(), None, prepare_worker_path, execute_worker_path);
		f(&mut config);
		let (host, task) = start(config, Metrics::default());
		let _ = handle.spawn(task);
		Self { cache_dir, host: Mutex::new(host) }
	}

	async fn precheck_pvf(
		&self,
		code: &[u8],
		executor_params: ExecutorParams,
	) -> Result<PrepareStats, PrepareError> {
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

fn host_prepare_kusama_runtime(c: &mut Criterion) {
	polkadot_node_core_pvf_common::sp_tracing::try_init_simple();

	let rt = tokio::runtime::Runtime::new().unwrap();

	let host = TestHost::new_with_config(rt.handle(), |cfg| {
		cfg.prepare_workers_hard_max_num = 1;
	});
	let cache_dir = host.cache_dir.as_path();

	let blob = staging_kusama_runtime::WASM_BINARY.unwrap();
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

	let mut group = c.benchmark_group("kusama");
	group.sampling_mode(SamplingMode::Flat);
	group.sample_size(20);
	group.measurement_time(Duration::from_secs(240));
	group.bench_function("host: prepare Kusama runtime", |b| {
		b.to_async(&rt).iter(|| async {
			// `PvfPrepData` is designed to be cheap to clone, so cloning shouldn't affect the
			// benchmark accuracy
			let _stats = host.precheck_pvf(&pvf.clone().code(), Default::default()).await.unwrap();

			// Delete the prepared artifact. Otherwise the next iterations will immediately finish.
			{
				// Get the artifact path (asserting it exists).
				let mut cache_dir: Vec<_> = std::fs::read_dir(cache_dir).unwrap().collect();
				assert_eq!(cache_dir.len(), 1);
				let artifact_path = cache_dir.pop().unwrap().unwrap();

				// Delete the artifact.
				std::fs::remove_file(artifact_path.path()).unwrap();
			}
		})
	});
	group.finish();
}

criterion_group!(preparation, host_prepare_kusama_runtime);
criterion_main!(preparation);
