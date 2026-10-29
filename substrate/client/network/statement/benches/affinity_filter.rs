//! Cost of the topic-affinity bloom filter by size: decoding a received filter, topic lookups,
//! statement matching, and matching one statement against every connected peer's filter.

use codec::{Decode, Encode};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sc_network_statement::affinity::AffinityFilter;
use sp_statement_store::Statement;
use std::hint::black_box;

const SEED: u64 = 0x42;
const QUERIES: usize = 4096;
const PEERS: usize = 50;
const MAX_HASHES: u32 = 64;
const FALSE_POS: f64 = 0.001;
const KIB: usize = 1024 / 8;

type Word = fn(&mut StdRng) -> u64;

fn all_ones(_: &mut StdRng) -> u64 {
	u64::MAX
}

fn random(rng: &mut StdRng) -> u64 {
	rng.gen()
}

fn dense(rng: &mut StdRng) -> u64 {
	!(0..6).fold(u64::MAX, |acc, _| acc & rng.gen::<u64>())
}

fn topics(rng: &mut StdRng, n: usize) -> Vec<[u8; 32]> {
	(0..n).map(|_| rng.gen()).collect()
}

fn real(rng: &mut StdRng, n: usize, false_pos: f64) -> (AffinityFilter, Vec<[u8; 32]>) {
	let members = topics(rng, n);
	(AffinityFilter::from_topics(members.iter(), rng.gen(), false_pos), members)
}

fn raw(rng: &mut StdRng, words: usize, word: Word) -> AffinityFilter {
	let bits = (0..words).map(|_| word(rng)).collect();
	AffinityFilter::from_raw_parts(rng.gen(), MAX_HASHES, bits)
}

fn statement(topics: &[[u8; 32]]) -> Statement {
	let mut s = Statement::new();
	s.set_plain_data(b"bench".to_vec());
	for (i, t) in topics.iter().enumerate() {
		s.set_topic(i, (*t).into());
	}
	s
}

fn label(words: usize) -> String {
	match words / KIB {
		kib if kib >= 1024 => format!("{}MiB", kib / 1024),
		kib => format!("{kib}KiB"),
	}
}

fn sizes(_: &mut Criterion) {
	let mut rng = StdRng::seed_from_u64(SEED);
	let topics_ = topics(&mut rng, 1_000_000);
	println!("topics  false_pos  encoded_bytes  hashes  bits_set  measured_false_pos");
	for (n, p) in [
		(128, 0.001),
		(1_000, 0.001),
		(10_000, 0.001),
		(10_000, 0.01),
		(18_000, 0.001),
		(27_000, 0.01),
		(50_000, 0.001),
	] {
		let (filter, members) = real(&mut rng, n, p);
		assert!(members.iter().all(|t| filter.contains(t)));
		let hits = topics_.iter().filter(|t| filter.contains(t)).count();
		let set: u32 = filter.bits().iter().map(|w| w.count_ones()).sum();
		println!(
			"{n:>6}  {p:>9}  {:>13}  {:>6}  {:>8.3}  {:>18.5}",
			filter.encode().len(),
			filter.num_hashes(),
			set as f64 / (filter.bits().len() * 64) as f64,
			hits as f64 / topics_.len() as f64,
		);
	}
}

fn decode(c: &mut Criterion) {
	let mut rng = StdRng::seed_from_u64(SEED);
	let mut group = c.benchmark_group("decode");
	for kib in [1, 12, 18, 32, 64, 128, 256, 512, 1024] {
		let bytes = raw(&mut rng, kib * KIB, random).encode();
		group.throughput(Throughput::Bytes(bytes.len() as u64));
		group.bench_with_input(
			BenchmarkId::from_parameter(label(kib * KIB)),
			&bytes,
			|b, bytes| b.iter(|| AffinityFilter::decode(&mut black_box(bytes.as_slice())).is_ok()),
		);
	}
	group.finish();
}

fn contains(c: &mut Criterion) {
	let mut rng = StdRng::seed_from_u64(SEED);
	let (f10k, m10k) = real(&mut rng, 10_000, FALSE_POS);
	let (f18k, m18k) = real(&mut rng, 18_000, FALSE_POS);
	let misses = topics(&mut rng, QUERIES);
	let mut pick = |m: &[[u8; 32]]| -> Vec<[u8; 32]> {
		(0..QUERIES).map(|_| m[rng.gen_range(0..m.len())]).collect()
	};
	let (hits10k, hits18k) = (pick(&m10k), pick(&m18k));
	let filters = [
		("10k_topics/member", f10k.clone(), hits10k),
		("10k_topics/random", f10k, misses.clone()),
		("18k_topics/member", f18k.clone(), hits18k),
		("18k_topics/random", f18k, misses.clone()),
		("32KiB_random_bits", raw(&mut rng, 32 * KIB, random), misses.clone()),
		("32KiB_all_ones", raw(&mut rng, 32 * KIB, all_ones), misses.clone()),
		("1MiB_all_ones", raw(&mut rng, 1024 * KIB, all_ones), misses.clone()),
		("match_all", AffinityFilter::match_all(rng.gen()), misses),
	];
	let mut group = c.benchmark_group("contains");
	for (name, filter, queries) in &filters {
		group.bench_function(*name, |b| {
			let mut i = 0;
			b.iter(|| {
				i = (i + 1) & (QUERIES - 1);
				filter.contains(black_box(&queries[i]))
			})
		});
	}
	group.finish();
}

fn matches_statement(c: &mut Criterion) {
	let mut rng = StdRng::seed_from_u64(SEED);
	let (filter, members) = real(&mut rng, 10_000, FALSE_POS);
	let one: Vec<_> = (0..QUERIES).map(|_| statement(&topics(&mut rng, 1))).collect();
	let four: Vec<_> = (0..QUERIES).map(|_| statement(&topics(&mut rng, 4))).collect();
	let member_last: Vec<_> = (0..QUERIES)
		.map(|_| {
			let mut t = topics(&mut rng, 3);
			t.push(members[rng.gen_range(0..members.len())]);
			statement(&t)
		})
		.collect();
	let mut group = c.benchmark_group("matches_statement_10k_topics");
	for (name, statements) in
		[("1_random_topic", one), ("4_random_topics", four), ("3_random_1_member", member_last)]
	{
		group.bench_function(name, |b| {
			let mut i = 0;
			b.iter(|| {
				i = (i + 1) & (QUERIES - 1);
				filter.matches_statement(black_box(&statements[i]))
			})
		});
	}
	group.finish();
}

fn propagation(c: &mut Criterion) {
	let mut rng = StdRng::seed_from_u64(SEED);
	let statements: Vec<_> = (0..QUERIES).map(|_| statement(&topics(&mut rng, 4))).collect();
	let mut sets: Vec<(String, Box<dyn Fn(&mut StdRng) -> AffinityFilter>)> =
		vec![("10k_topics".into(), Box::new(|rng: &mut StdRng| real(rng, 10_000, FALSE_POS).0))];
	for (kind, word) in [("random_bits", random as Word), ("all_ones", all_ones), ("dense", dense)]
	{
		let sizes: &[usize] =
			if kind == "dense" { &[12, 32, 64, 128, 256, 512, 1024] } else { &[32, 1024] };
		for &kib in sizes {
			sets.push((
				format!("{}_{kind}", label(kib * KIB)),
				Box::new(move |rng: &mut StdRng| raw(rng, kib * KIB, word)),
			));
		}
	}
	let mut group = c.benchmark_group("propagation_50_peers");
	for (name, make) in &sets {
		let peers: Vec<AffinityFilter> = (0..PEERS).map(|_| make(&mut rng)).collect();
		group.bench_function(name.as_str(), |b| {
			let mut i = 0;
			b.iter(|| {
				i = (i + 1) & (QUERIES - 1);
				let s = black_box(&statements[i]);
				peers.iter().filter(|f| f.matches_statement(s)).count()
			})
		});
	}
	group.finish();
}

criterion_group!(benches, sizes, decode, contains, matches_statement, propagation);
criterion_main!(benches);
