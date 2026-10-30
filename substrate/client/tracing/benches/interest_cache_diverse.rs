// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Benchmark diverse targets with interest cache configuration from INTEREST_CACHE env var.
//!
//! Usage:
//! ```
//! INTEREST_CACHE=lru_cache_size=1024,min_verbosity=debug cargo bench --bench interest_cache_diverse
//! INTEREST_CACHE=default cargo bench --bench interest_cache_diverse
//! INTEREST_CACHE=disabled cargo bench --bench interest_cache_diverse
//! ```

use criterion::{criterion_group, criterion_main, Criterion};

mod common;

fn bench_diverse_targets(c: &mut Criterion) {
	common::init_logger();
	c.bench_function("diverse_targets", |b| {
		b.iter(|| {
			for i in 0..1000 {
				let target = format!("module_{}", i % 50);
				log::debug!(target: &target, "message {}", i);
				log::trace!(target: &target, "trace {}", i);
			}
		})
	});
}

criterion_group!(benches, bench_diverse_targets);
criterion_main!(benches);
