// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sp_core::crypto::Pair as _;

fn bench_ed25519(c: &mut Criterion) {
	let mut group = c.benchmark_group("ed25519");

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::ed25519::Pair::generate().0;
		group.bench_function(BenchmarkId::new("signing", format!("{}", msg_size)), |b| {
			b.iter(|| key.sign(&msg))
		});
	}

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::ed25519::Pair::generate().0;
		let sig = key.sign(&msg);
		let public = key.public();
		group.bench_function(BenchmarkId::new("verifying", format!("{}", msg_size)), |b| {
			b.iter(|| sp_core::ed25519::Pair::verify(&sig, &msg, &public))
		});
	}

	group.finish();
}

fn bench_sr25519(c: &mut Criterion) {
	let mut group = c.benchmark_group("sr25519");

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::sr25519::Pair::generate().0;
		group.bench_function(BenchmarkId::new("signing", format!("{}", msg_size)), |b| {
			b.iter(|| key.sign(&msg))
		});
	}

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::sr25519::Pair::generate().0;
		let sig = key.sign(&msg);
		let public = key.public();
		group.bench_function(BenchmarkId::new("verifying", format!("{}", msg_size)), |b| {
			b.iter(|| sp_core::sr25519::Pair::verify(&sig, &msg, &public))
		});
	}

	group.finish();
}

fn bench_ecdsa(c: &mut Criterion) {
	let mut group = c.benchmark_group("ecdsa");

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::ecdsa::Pair::generate().0;
		group.bench_function(BenchmarkId::new("signing", format!("{}", msg_size)), |b| {
			b.iter(|| key.sign(&msg))
		});
	}

	for &msg_size in &[32, 1024, 1024 * 1024] {
		let msg = (0..msg_size).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
		let key = sp_core::ecdsa::Pair::generate().0;
		let sig = key.sign(&msg);
		let public = key.public();
		group.bench_function(BenchmarkId::new("verifying", format!("{}", msg_size)), |b| {
			b.iter(|| sp_core::ecdsa::Pair::verify(&sig, &msg, &public))
		});
	}

	group.finish();
}

criterion_group!(benches, bench_ed25519, bench_sr25519, bench_ecdsa,);
criterion_main!(benches);
