// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Benchmark multithreaded logging with interest cache configuration from INTEREST_CACHE env var.
//!
//! Usage:
//! ```
//! INTEREST_CACHE=lru_cache_size=1024,min_verbosity=debug cargo bench --bench interest_cache_multithreaded
//! INTEREST_CACHE=default cargo bench --bench interest_cache_multithreaded
//! INTEREST_CACHE=disabled cargo bench --bench interest_cache_multithreaded
//! ```

use criterion::{criterion_group, criterion_main, Criterion};

mod common;

fn bench_multithreaded(c: &mut Criterion) {
	common::init_logger();
	let mut group = c.benchmark_group("multithreaded");

	group.bench_function("8_threads", |b| {
		b.iter(|| {
			let handles: Vec<_> = (0..8)
				.map(|thread_id| {
					std::thread::spawn(move || {
						for i in 0..1000 {
							log::debug!(target: "substrate", "thread {} msg {}", thread_id, i);
							log::trace!(target: "runtime", "thread {} trace {}", thread_id, i);
						}
					})
				})
				.collect();
			for handle in handles {
				handle.join().unwrap();
			}
		})
	});
	group.finish();
}

criterion_group!(benches, bench_multithreaded);
criterion_main!(benches);
