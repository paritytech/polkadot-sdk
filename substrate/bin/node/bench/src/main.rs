// This file is part of Substrate.

// Copyright (C) 2020 Parity Technologies (UK) Ltd.
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

#[macro_use] mod core;
mod import;
mod trie;
mod simple_trie;
mod generator;
mod tempdb;
mod state_sizes;

use crate::core::{run_benchmark, Mode as BenchmarkMode};
use crate::tempdb::DatabaseType;
use import::{ImportBenchmarkDescription, SizeType};
use trie::{TrieReadBenchmarkDescription, TrieWriteBenchmarkDescription, DatabaseSize};
use node_testing::bench::{Profile, KeyTypes, BlockType, DatabaseType as BenchDataBaseType};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "node-bench", about = "Node integration benchmarks")]
struct Opt {
	/// Show list of all available benchmarks.
	///
	/// Will output ("name", "path"). Benchmarks can then be filtered by path.
	#[structopt(short, long)]
	list: bool,

	/// Machine readable json output.
	///
	/// This also suppresses all regular output (except to stderr)
	#[structopt(short, long)]
	json: bool,

	/// Filter benchmarks.
	///
	/// Run with `--list` for the hint of what to filter.
	filter: Option<String>,

	/// Number of transactions for block import with `custom` size.
	#[structopt(long)]
	transactions: Option<usize>,

	/// Mode
	///
	/// "regular" for regular benchmark
	///
	/// "profile" mode adds pauses between measurable runs,
	/// so that actual interval can be selected in the profiler of choice.
	#[structopt(short, long, default_value = "regular")]
	mode: BenchmarkMode,
}

fn main() {
	let opt = Opt::from_args();

	if !opt.json {
		sc_cli::init_logger("");
	}

	let mut import_benchmarks = Vec::new();

	for profile in [Profile::Wasm, Profile::Native].iter() {
		for size in [
			SizeType::Empty,
			SizeType::Small,
			SizeType::Medium,
			SizeType::Large,
			SizeType::Full,
			SizeType::Custom(opt.transactions.unwrap_or(0)),
		].iter() {
			for block_type in [
				BlockType::RandomTransfersKeepAlive,
				BlockType::RandomTransfersReaping,
				BlockType::Noop,
			].iter() {
				for database_type in [BenchDataBaseType::RocksDb, BenchDataBaseType::ParityDb].iter() {
					import_benchmarks.push((profile, size.clone(), block_type.clone(), database_type));
				}
			}
		}
	}

	let benchmarks = matrix!(
		(profile, size, block_type, database_type) in import_benchmarks.into_iter() =>
			ImportBenchmarkDescription {
				profile: *profile,
				key_types: KeyTypes::Sr25519,
				size: size,
				block_type: block_type,
				database_type: *database_type,
			},
		(size, db_type) in
			[
				DatabaseSize::Empty, DatabaseSize::Smallest, DatabaseSize::Small,
				DatabaseSize::Medium, DatabaseSize::Large, DatabaseSize::Huge,
			]
			.iter().flat_map(|size|
			[
				DatabaseType::RocksDb, DatabaseType::ParityDb
			]
			.iter().map(move |db_type| (size, db_type)))
			=> TrieReadBenchmarkDescription { database_size: *size, database_type: *db_type },
		(size, db_type) in
			[
				DatabaseSize::Empty, DatabaseSize::Smallest, DatabaseSize::Small,
				DatabaseSize::Medium, DatabaseSize::Large, DatabaseSize::Huge,
			]
			.iter().flat_map(|size|
			[
				DatabaseType::RocksDb, DatabaseType::ParityDb
			]
			.iter().map(move |db_type| (size, db_type)))
			=> TrieWriteBenchmarkDescription { database_size: *size, database_type: *db_type },
	);

	if opt.list {
		println!("Available benchmarks:");
		if let Some(filter) = opt.filter.as_ref() {
			println!("\t(filtered by \"{}\")", filter);
		}
		for benchmark in benchmarks.iter() {
			if opt.filter.as_ref().map(|f| benchmark.path().has(f)).unwrap_or(true) {
				println!("{}: {}", benchmark.name(), benchmark.path().full())
			}
		}
		return;
	}

	let mut results = Vec::new();
	for benchmark in benchmarks {
		if opt.filter.as_ref().map(|f| benchmark.path().has(f)).unwrap_or(true) {
			log::info!("Starting {}", benchmark.name());
			let result = run_benchmark(benchmark, opt.mode);
			log::info!("{}", result);

			results.push(result);
		}
	}

	if opt.json {
		let json_result: String = serde_json::to_string(&results).expect("Failed to construct json");
		println!("{}", json_result);
	}
}
