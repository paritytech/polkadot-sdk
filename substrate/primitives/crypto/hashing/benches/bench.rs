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

use criterion::{black_box, criterion_group, criterion_main, Bencher, BenchmarkId, Criterion};

// Min 32 bytes buffer
const MIN_EXP: usize = 5;
// Max 1 MB buffer
const MAX_EXP: usize = 20;

fn bench_blake2_128(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::blake2_128(black_box(buf));
	});
}

fn bench_twox_128(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::twox_128(black_box(buf));
	});
}

fn bench_blake2_256(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::blake2_256(black_box(buf));
	});
}

fn bench_twox_256(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::twox_256(black_box(buf));
	});
}

fn bench_sha_256(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::sha2_256(black_box(buf));
	});
}

fn bench_keccak_256(b: &mut Bencher, buf: &Vec<u8>) {
	b.iter(|| {
		let _a = sp_crypto_hashing::keccak_256(black_box(buf));
	});
}

fn bench_hash(c: &mut Criterion) {
	let mut group = c.benchmark_group("hashing-128");
	let buf = vec![0u8; 1 << MAX_EXP];

	for i in MIN_EXP..=MAX_EXP {
		let size = 1 << i;
		group.bench_with_input(BenchmarkId::new("blake2-128", size), &buf, bench_blake2_128);
		group.bench_with_input(BenchmarkId::new("twox-128", size), &buf, bench_twox_128);
	}
	group.finish();

	let mut group = c.benchmark_group("hashing-256");
	for i in MIN_EXP..=MAX_EXP {
		let size = 1 << i;
		group.bench_with_input(BenchmarkId::new("blake2-256", size), &buf, bench_blake2_256);
		group.bench_with_input(BenchmarkId::new("twox-256", size), &buf, bench_twox_256);
		group.bench_with_input(BenchmarkId::new("sha-256", size), &buf, bench_sha_256);
		group.bench_with_input(BenchmarkId::new("keccak-256", size), &buf, bench_keccak_256);
	}
	group.finish();
}

criterion_group!(benches, bench_hash);
criterion_main!(benches);
