// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Benchmark realistic logging with interest cache configuration from INTEREST_CACHE env var.
//!
//! Usage:
//! ```
//! INTEREST_CACHE=lru_cache_size=1024,min_verbosity=debug cargo bench --bench interest_cache_realistic
//! INTEREST_CACHE=default cargo bench --bench interest_cache_realistic
//! INTEREST_CACHE=disabled cargo bench --bench interest_cache_realistic
//! ```

use criterion::{criterion_group, criterion_main, Criterion};

mod common;

fn bench_realistic_logging(c: &mut Criterion) {
	common::init_logger();
	c.bench_function("realistic_logging", |b| {
		b.iter(|| {
			for i in 0..1000 {
				log::trace!(target: "substrate", "trace message {}", i);
				log::debug!(target: "runtime", "debug message {}", i);
				log::debug!(target: "sync", "sync debug {}", i);
				log::trace!(target: "consensus", "consensus trace {}", i);
				log::debug!(target: "network", "network debug {}", i);
				if i % 10 == 0 {
					log::info!(target: "substrate", "info message {}", i);
				}
			}
		})
	});
}

criterion_group!(benches, bench_realistic_logging);
criterion_main!(benches);
