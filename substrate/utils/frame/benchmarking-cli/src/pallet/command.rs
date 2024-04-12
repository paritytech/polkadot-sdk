// This file is part of Substrate.

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

use super::{
	types::{ComponentRange, ComponentRangeMap},
	writer, ListOutput, PalletCmd,
};
use crate::pallet::{types::FetchedCode, GenesisBuilder};
use codec::{Decode, Encode};
use frame_benchmarking::{
	Analysis, BenchmarkBatch, BenchmarkBatchSplitResults, BenchmarkList, BenchmarkParameter,
	BenchmarkResult, BenchmarkSelector,
};
use frame_support::traits::StorageInfo;
use linked_hash_map::LinkedHashMap;
use sc_chain_spec::json_patch::merge as json_merge;
use sc_cli::{execution_method_from_cli, ChainSpec, CliConfiguration, Result, SharedParams};
use sc_client_db::BenchmarkingState;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sp_core::{
	offchain::{
		testing::{TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	traits::{CallContext, CodeExecutor, ReadRuntimeVersionExt, WrappedRuntimeCode},
};
use sp_externalities::Extensions;
use sp_genesis_builder::{PresetId, Result as GenesisBuildResult};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::traits::Hash;
use sp_state_machine::{OverlayedChanges, StateMachine};
use sp_wasm_interface::HostFunctions;
use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet, HashMap},
	fmt::Debug,
	fs,
	str::FromStr,
	time,
};

/// Logging target
const LOG_TARGET: &'static str = "polkadot_sdk_frame::benchmark::pallet";

/// How the PoV size of a storage item should be estimated.
#[derive(clap::ValueEnum, Debug, Eq, PartialEq, Clone, Copy)]
pub enum PovEstimationMode {
	/// Use the maximal encoded length as provided by [`codec::MaxEncodedLen`].
	MaxEncodedLen,
	/// Measure the accessed value size in the pallet benchmarking and add some trie overhead.
	Measured,
	/// Do not estimate the PoV size for this storage item or benchmark.
	Ignored,
}

impl FromStr for PovEstimationMode {
	type Err = &'static str;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		match s {
			"MaxEncodedLen" => Ok(Self::MaxEncodedLen),
			"Measured" => Ok(Self::Measured),
			"Ignored" => Ok(Self::Ignored),
			_ => unreachable!("The benchmark! macro should have prevented this"),
		}
	}
}

/// Maps (pallet, benchmark) -> ((pallet, storage) -> PovEstimationMode)
pub(crate) type PovModesMap =
	HashMap<(String, String), HashMap<(String, String), PovEstimationMode>>;

#[derive(Debug, Clone)]
struct SelectedBenchmark {
	pallet: String,
	extrinsic: String,
	components: Vec<(BenchmarkParameter, u32, u32)>,
	pov_modes: Vec<(String, String)>,
}

// This takes multiple benchmark batches and combines all the results where the pallet, instance,
// and benchmark are the same.
fn combine_batches(
	time_batches: Vec<BenchmarkBatch>,
	db_batches: Vec<BenchmarkBatch>,
) -> Vec<BenchmarkBatchSplitResults> {
	if time_batches.is_empty() && db_batches.is_empty() {
		return Default::default()
	}

	let mut all_benchmarks =
		LinkedHashMap::<_, (Vec<BenchmarkResult>, Vec<BenchmarkResult>)>::new();

	db_batches
		.into_iter()
		.for_each(|BenchmarkBatch { pallet, instance, benchmark, results }| {
			// We use this key to uniquely identify a benchmark among batches.
			let key = (pallet, instance, benchmark);

			match all_benchmarks.get_mut(&key) {
				// We already have this benchmark, so we extend the results.
				Some(x) => x.1.extend(results),
				// New benchmark, so we add a new entry with the initial results.
				None => {
					all_benchmarks.insert(key, (Vec::new(), results));
				},
			}
		});

	time_batches
		.into_iter()
		.for_each(|BenchmarkBatch { pallet, instance, benchmark, results }| {
			// We use this key to uniquely identify a benchmark among batches.
			let key = (pallet, instance, benchmark);

			match all_benchmarks.get_mut(&key) {
				// We already have this benchmark, so we extend the results.
				Some(x) => x.0.extend(results),
				None => panic!("all benchmark keys should have been populated by db batches"),
			}
		});

	all_benchmarks
		.into_iter()
		.map(|((pallet, instance, benchmark), (time_results, db_results))| {
			BenchmarkBatchSplitResults { pallet, instance, benchmark, time_results, db_results }
		})
		.collect::<Vec<_>>()
}

/// Explains possible reasons why the metadata for the benchmarking could not be found.
const ERROR_METADATA_NOT_FOUND: &'static str = "Did not find the benchmarking metadata. \
This could mean that you either did not build the node correctly with the \
`--features runtime-benchmarks` flag, or the chain spec that you are using was \
not created by a node that was compiled with the flag";

/// When the runtime could not build the genesis storage.
const ERROR_CANNOT_BUILD_GENESIS: &str = "The runtime returned \
an error when trying to build the genesis storage. Please ensure that all pallets \
define a genesis config that can be built. This can be tested with: \
https://github.com/paritytech/polkadot-sdk/pull/3412";

/// Warn when using the chain spec to generate the genesis state.
const WARN_SPEC_GENESIS_CTOR: &'static str = "Using the chain spec instead of the runtime to \
generate the genesis state is deprecated. Please remove the `--chain`/`--dev`/`--local` argument, \
point `--runtime` to your runtime blob and set `--genesis-builder=runtime`. This warning may \
become a hard error any time after December 2024.";

/// The preset that we expect to find in the GenesisBuilder runtime API.
const GENESIS_PRESET: &str = "development";

impl PalletCmd {
	/// Runs the command and benchmarks a pallet.
	#[deprecated(
		note = "`run` will be removed after December 2024. Use `run_with_spec` instead or \
	completely remove the code and use the `frame-benchmarking-cli` instead (see \
	https://github.com/paritytech/polkadot-sdk/pull/3512)."
	)]
	pub fn run<Hasher, ExtraHostFunctions>(&self, config: sc_service::Configuration) -> Result<()>
	where
		Hasher: Hash,
		ExtraHostFunctions: HostFunctions,
	{
		self.run_with_spec::<Hasher, ExtraHostFunctions>(Some(config.chain_spec))
	}

	/// Runs the pallet benchmarking command.
	pub fn run_with_spec<Hasher, ExtraHostFunctions>(
		&self,
		chain_spec: Option<Box<dyn ChainSpec>>,
	) -> Result<()>
	where
		Hasher: Hash,
		ExtraHostFunctions: HostFunctions,
	{
		self.check_args()?;
		let _d = self.execution.as_ref().map(|exec| {
			// We print the error at the end, since there is often A LOT of output.
			sp_core::defer::DeferGuard::new(move || {
				log::error!(
					target: LOG_TARGET,
					"⚠️  Argument `--execution` is deprecated. Its value of `{exec}` has on effect.",
				)
			})
		});

		if let Some(json_input) = &self.json_input {
			let raw_data = match std::fs::read(json_input) {
				Ok(raw_data) => raw_data,
				Err(error) =>
					return Err(format!("Failed to read {:?}: {}", json_input, error).into()),
			};
			let batches: Vec<BenchmarkBatchSplitResults> = match serde_json::from_slice(&raw_data) {
				Ok(batches) => batches,
				Err(error) =>
					return Err(format!("Failed to deserialize {:?}: {}", json_input, error).into()),
			};
			return self.output_from_results(&batches)
		}

		let (genesis_storage, genesis_changes) =
			self.genesis_storage::<Hasher, ExtraHostFunctions>(&chain_spec)?;
		let mut changes = genesis_changes.clone();

		let cache_size = Some(self.database_cache_size as usize);
		let state_with_tracking = BenchmarkingState::<Hasher>::new(
			genesis_storage.clone(),
			cache_size,
			// Record proof size
			true,
			// Enable storage tracking
			true,
		)?;
		let state_without_tracking = BenchmarkingState::<Hasher>::new(
			genesis_storage,
			cache_size,
			// Do not record proof size
			false,
			// Do not enable storage tracking
			false,
		)?;

		let method =
			execution_method_from_cli(self.wasm_method, self.wasmtime_instantiation_strategy);

		let state = &state_without_tracking;
		let runtime = self.runtime_blob(&state_without_tracking)?;
		let runtime_code = runtime.code()?;
		let alloc_strategy = Self::alloc_strategy(runtime_code.heap_pages);

		let executor = WasmExecutor::<(
			sp_io::SubstrateHostFunctions,
			frame_benchmarking::benchmarking::HostFunctions,
			ExtraHostFunctions,
		)>::builder()
		.with_execution_method(method)
		.with_allow_missing_host_functions(self.allow_missing_host_functions)
		.with_onchain_heap_alloc_strategy(alloc_strategy)
		.with_offchain_heap_alloc_strategy(alloc_strategy)
		.with_max_runtime_instances(2)
		.with_runtime_cache_size(2)
		.build();

		let (list, storage_info): (Vec<BenchmarkList>, Vec<StorageInfo>) =
			Self::exec_state_machine(
				StateMachine::new(
					state,
					&mut changes,
					&executor,
					"Benchmark_benchmark_metadata",
					&(self.extra).encode(),
					&mut Self::build_extensions(executor.clone()),
					&runtime_code,
					CallContext::Offchain,
				),
				ERROR_METADATA_NOT_FOUND,
			)?;

		// Use the benchmark list and the user input to determine the set of benchmarks to run.
		let benchmarks_to_run = self.select_benchmarks_to_run(list)?;

		if let Some(list_output) = self.list {
			list_benchmark(benchmarks_to_run, list_output, self.no_csv_header);
			return Ok(())
		}

		// Run the benchmarks
		let mut batches = Vec::new();
		let mut batches_db = Vec::new();
		let mut timer = time::SystemTime::now();
		// Maps (pallet, extrinsic) to its component ranges.
		let mut component_ranges = HashMap::<(String, String), Vec<ComponentRange>>::new();
		let pov_modes = Self::parse_pov_modes(&benchmarks_to_run)?;
		let mut failed = Vec::<(String, String)>::new();

		'outer: for (i, SelectedBenchmark { pallet, extrinsic, components, .. }) in
			benchmarks_to_run.clone().into_iter().enumerate()
		{
			log::info!(
				target: LOG_TARGET,
				"[{: >3} % ] Starting benchmark: {pallet}::{extrinsic}",
				(i * 100) / benchmarks_to_run.len(),
			);
			let all_components = if components.is_empty() {
				vec![Default::default()]
			} else {
				let mut all_components = Vec::new();
				for (idx, (name, low, high)) in components.iter().enumerate() {
					let lowest = self.lowest_range_values.get(idx).cloned().unwrap_or(*low);
					let highest = self.highest_range_values.get(idx).cloned().unwrap_or(*high);

					let diff =
						highest.checked_sub(lowest).ok_or("`low` cannot be higher than `high`")?;

					// The slope logic needs at least two points
					// to compute a slope.
					if self.steps < 2 {
						return Err("`steps` must be at least 2.".into())
					}

					let step_size = (diff as f32 / (self.steps - 1) as f32).max(0.0);

					for s in 0..self.steps {
						// This is the value we will be testing for component `name`
						let component_value =
							((lowest as f32 + step_size * s as f32) as u32).clamp(lowest, highest);

						// Select the max value for all the other components.
						let c: Vec<(BenchmarkParameter, u32)> = components
							.iter()
							.enumerate()
							.map(|(idx, (n, _, h))| {
								if n == name {
									(*n, component_value)
								} else {
									(*n, *self.highest_range_values.get(idx).unwrap_or(h))
								}
							})
							.collect();
						all_components.push(c);
					}

					component_ranges
						.entry((pallet.clone(), extrinsic.clone()))
						.or_default()
						.push(ComponentRange { name: name.to_string(), min: lowest, max: highest });
				}
				all_components
			};
			for (s, selected_components) in all_components.iter().enumerate() {
				// First we run a verification
				if !self.no_verify {
					let mut changes = genesis_changes.clone();
					let state = &state_without_tracking;
					// Don't use these results since verification code will add overhead.
					let _batch: Vec<BenchmarkBatch> = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state,
							&mut changes,
							&executor,
							"Benchmark_dispatch_benchmark",
							&(
								pallet.as_bytes(),
								extrinsic.as_bytes(),
								&selected_components.clone(),
								true, // run verification code
								1,    // no need to do internal repeats
							)
								.encode(),
							&mut Self::build_extensions(executor.clone()),
							&runtime_code,
							CallContext::Offchain,
						),
						"dispatch a benchmark",
					) {
						Err(e) => {
							log::error!("Error executing and verifying runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Err(e)) => {
							log::error!("Error executing and verifying runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Ok(b)) => b,
					};
				}
				// Do one loop of DB tracking.
				{
					let mut changes = genesis_changes.clone();
					let state = &state_with_tracking;
					let batch: Vec<BenchmarkBatch> = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state, // todo remove tracking
							&mut changes,
							&executor,
							"Benchmark_dispatch_benchmark",
							&(
								pallet.as_bytes(),
								extrinsic.as_bytes(),
								&selected_components.clone(),
								false, // don't run verification code for final values
								self.repeat,
							)
								.encode(),
							&mut Self::build_extensions(executor.clone()),
							&runtime_code,
							CallContext::Offchain,
						),
						"dispatch a benchmark",
					) {
						Err(e) => {
							log::error!("Error executing runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Err(e)) => {
							log::error!("Benchmark {pallet}::{extrinsic} failed: {e}",);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Ok(b)) => b,
					};

					batches_db.extend(batch);
				}
				// Finally run a bunch of loops to get extrinsic timing information.
				for r in 0..self.external_repeat {
					let mut changes = genesis_changes.clone();
					let state = &state_without_tracking;
					let batch = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state, // todo remove tracking
							&mut changes,
							&executor,
							"Benchmark_dispatch_benchmark",
							&(
								pallet.as_bytes(),
								extrinsic.as_bytes(),
								&selected_components.clone(),
								false, // don't run verification code for final values
								self.repeat,
							)
								.encode(),
							&mut Self::build_extensions(executor.clone()),
							&runtime_code,
							CallContext::Offchain,
						),
						"dispatch a benchmark",
					) {
						Err(e) => {
							return Err(format!("Error executing runtime benchmark: {e}",).into());
						},
						Ok(Err(e)) => {
							return Err(
								format!("Benchmark {pallet}::{extrinsic} failed: {e}",).into()
							);
						},
						Ok(Ok(b)) => b,
					};

					batches.extend(batch);

					// Show progress information
					if let Ok(elapsed) = timer.elapsed() {
						if elapsed >= time::Duration::from_secs(5) {
							timer = time::SystemTime::now();

							log::info!(
								target: LOG_TARGET,
								"[{: >3} % ] Running  benchmark: {pallet}::{extrinsic}({} args) {}/{} {}/{}",
								(i * 100) / benchmarks_to_run.len(),
								components.len(),
								s + 1, // s starts at 0.
								all_components.len(),
								r + 1,
								self.external_repeat,
							);
						}
					}
				}
			}
		}

		assert!(batches_db.len() == batches.len() / self.external_repeat as usize);

		if !failed.is_empty() {
			failed.sort();
			eprintln!(
				"The following {} benchmarks failed:\n{}",
				failed.len(),
				failed.iter().map(|(p, e)| format!("- {p}::{e}")).collect::<Vec<_>>().join("\n")
			);
			return Err(format!("{} benchmarks failed", failed.len()).into())
		}

		// Combine all of the benchmark results, so that benchmarks of the same pallet/function
		// are together.
		let batches = combine_batches(batches, batches_db);
		self.output(&batches, &storage_info, &component_ranges, pov_modes)
	}

	fn select_benchmarks_to_run(&self, list: Vec<BenchmarkList>) -> Result<Vec<SelectedBenchmark>> {
		let pallet = self.pallet.clone().unwrap_or_default();
		let pallet = pallet.as_bytes();

		let extrinsic = self.extrinsic.clone().unwrap_or_default();
		let extrinsic_split: Vec<&str> = extrinsic.split(',').collect();
		let extrinsics: Vec<_> = extrinsic_split.iter().map(|x| x.trim().as_bytes()).collect();

		// Use the benchmark list and the user input to determine the set of benchmarks to run.
		let mut benchmarks_to_run = Vec::new();
		list.iter()
			.filter(|item| pallet.is_empty() || pallet == &b"*"[..] || pallet == &item.pallet[..])
			.for_each(|item| {
				for benchmark in &item.benchmarks {
					let benchmark_name = &benchmark.name;
					if extrinsic.is_empty() ||
						extrinsic.as_bytes() == &b"*"[..] ||
						extrinsics.contains(&&benchmark_name[..])
					{
						benchmarks_to_run.push((
							item.pallet.clone(),
							benchmark.name.clone(),
							benchmark.components.clone(),
							benchmark.pov_modes.clone(),
						))
					}
				}
			});
		// Convert `Vec<u8>` to `String` for better readability.
		let benchmarks_to_run: Vec<_> = benchmarks_to_run
			.into_iter()
			.map(|(pallet, extrinsic, components, pov_modes)| {
				let pallet = String::from_utf8(pallet.clone()).expect("Encoded from String; qed");
				let extrinsic =
					String::from_utf8(extrinsic.clone()).expect("Encoded from String; qed");

				SelectedBenchmark {
					pallet,
					extrinsic,
					components,
					pov_modes: pov_modes
						.into_iter()
						.map(|(p, s)| {
							(String::from_utf8(p).unwrap(), String::from_utf8(s).unwrap())
						})
						.collect(),
				}
			})
			.collect();

		if benchmarks_to_run.is_empty() {
			return Err("No benchmarks found which match your input.".into())
		}

		Ok(benchmarks_to_run)
	}

	/// Produce a genesis storage and genesis changes.
	///
	/// It would be easier to only return one type, but there is no easy way to convert them.
	// TODO: Re-write `BenchmarkingState` to not be such a clusterfuck and only accept
	// `OverlayedChanges` instead of a mix between `OverlayedChanges` and `State`. But this can only
	// be done once we deprecated and removed the legacy interface :(
	fn genesis_storage<H: Hash, F: HostFunctions>(
		&self,
		chain_spec: &Option<Box<dyn ChainSpec>>,
	) -> Result<(sp_storage::Storage, OverlayedChanges<H>)> {
		Ok(match (self.genesis_builder, self.runtime.is_some()) {
			(Some(GenesisBuilder::None), _) => Default::default(),
			(Some(GenesisBuilder::Spec), _) | (None, false) => {
				log::warn!("{WARN_SPEC_GENESIS_CTOR}");
				let Some(chain_spec) = chain_spec else {
					return Err("No chain spec specified to generate the genesis state".into());
				};

				let storage = chain_spec
					.build_storage()
					.map_err(|e| format!("{ERROR_CANNOT_BUILD_GENESIS}\nError: {e}"))?;

				(storage, Default::default())
			},
			(Some(GenesisBuilder::Runtime), _) | (None, true) =>
				(Default::default(), self.genesis_from_runtime::<H, F>()?),
		})
	}

	/// Generate the genesis changeset by the runtime API.
	fn genesis_from_runtime<H: Hash, F: HostFunctions>(&self) -> Result<OverlayedChanges<H>> {
		let state = BenchmarkingState::<H>::new(
			Default::default(),
			Some(self.database_cache_size as usize),
			false,
			false,
		)?;

		// Create a dummy WasmExecutor just to build the genesis storage.
		let method =
			execution_method_from_cli(self.wasm_method, self.wasmtime_instantiation_strategy);
		let executor = WasmExecutor::<(
			sp_io::SubstrateHostFunctions,
			frame_benchmarking::benchmarking::HostFunctions,
			F,
		)>::builder()
		.with_execution_method(method)
		.with_allow_missing_host_functions(self.allow_missing_host_functions)
		.build();

		let runtime = self.runtime_blob(&state)?;
		let runtime_code = runtime.code()?;

		// We cannot use the `GenesisConfigBuilderRuntimeCaller` here since it returns the changes
		// as `Storage` item, but we need it as `OverlayedChanges`.
		let genesis_json: Option<Vec<u8>> = Self::exec_state_machine(
			StateMachine::new(
				&state,
				&mut Default::default(),
				&executor,
				"GenesisBuilder_get_preset",
				&None::<PresetId>.encode(), // Use the default preset
				&mut Self::build_extensions(executor.clone()),
				&runtime_code,
				CallContext::Offchain,
			),
			"build the genesis spec",
		)?;

		let Some(base_genesis_json) = genesis_json else {
			return Err("GenesisBuilder::get_preset returned no data".into())
		};

		let base_genesis_json = serde_json::from_slice::<serde_json::Value>(&base_genesis_json)
			.map_err(|e| format!("GenesisBuilder::get_preset returned invalid JSON: {:?}", e))?;

		let dev_genesis_json: Option<Vec<u8>> = Self::exec_state_machine(
			StateMachine::new(
				&state,
				&mut Default::default(),
				&executor,
				"GenesisBuilder_get_preset",
				&Some::<PresetId>(GENESIS_PRESET.into()).encode(), // Use the default preset
				&mut Self::build_extensions(executor.clone()),
				&runtime_code,
				CallContext::Offchain,
			),
			"build the genesis spec",
		)?;

		let mut genesis_json = serde_json::Value::default();
		json_merge(&mut genesis_json, base_genesis_json);

		if let Some(dev) = dev_genesis_json {
			let dev: serde_json::Value = serde_json::from_slice(&dev).map_err(|e| {
				format!("GenesisBuilder::get_preset returned invalid JSON: {:?}", e)
			})?;
			json_merge(&mut genesis_json, dev);
		} else {
			log::warn!(
				"Could not find genesis preset '{GENESIS_PRESET}'. Falling back to default."
			);
		}

		let json_pretty_str = serde_json::to_string_pretty(&genesis_json)
			.map_err(|e| format!("json to string failed: {e}"))?;

		let mut changes = Default::default();
		let build_res: GenesisBuildResult = Self::exec_state_machine(
			StateMachine::new(
				&state,
				&mut changes,
				&executor,
				"GenesisBuilder_build_state",
				&json_pretty_str.encode(),
				&mut Extensions::default(),
				&runtime_code,
				CallContext::Offchain,
			),
			"populate the genesis state",
		)?;

		if let Err(e) = build_res {
			return Err(format!("GenesisBuilder::build_state failed: {}", e).into())
		}

		Ok(changes)
	}

	/// Execute a state machine and decode its return value as `R`.
	fn exec_state_machine<R: Decode, H: Hash, Exec: CodeExecutor>(
		mut machine: StateMachine<BenchmarkingState<H>, H, Exec>,
		hint: &str,
	) -> Result<R> {
		let res = machine
			.execute()
			.map_err(|e| format!("Could not call runtime API to {hint}: {}", e))?;
		let res = R::decode(&mut &res[..])
			.map_err(|e| format!("Failed to decode runtime API result to {hint}: {:?}", e))?;
		Ok(res)
	}

	/// Build the extension that are available for pallet benchmarks.
	fn build_extensions<E: CodeExecutor>(exe: E) -> Extensions {
		let mut extensions = Extensions::default();
		let (offchain, _) = TestOffchainExt::new();
		let (pool, _) = TestTransactionPoolExt::new();
		let keystore = MemoryKeystore::new();
		extensions.register(KeystoreExt::new(keystore));
		extensions.register(OffchainWorkerExt::new(offchain.clone()));
		extensions.register(OffchainDbExt::new(offchain));
		extensions.register(TransactionPoolExt::new(pool));
		extensions.register(ReadRuntimeVersionExt::new(exe));
		extensions
	}

	/// Load the runtime blob for this benchmark.
	///
	/// The blob will either be loaded from the `:code` key out of the chain spec, or from a file
	/// when specified with `--runtime`.
	fn runtime_blob<'a, H: Hash>(
		&self,
		state: &'a BenchmarkingState<H>,
	) -> Result<FetchedCode<'a, BenchmarkingState<H>, H>> {
		if let Some(runtime) = &self.runtime {
			log::info!("Loading WASM from {}", runtime.display());
			let code = fs::read(runtime)?;
			let hash = sp_core::blake2_256(&code).to_vec();
			let wrapped_code = WrappedRuntimeCode(Cow::Owned(code));

			Ok(FetchedCode::FromFile { wrapped_code, heap_pages: self.heap_pages, hash })
		} else {
			log::info!("Loading WASM from genesis state");
			let state = sp_state_machine::backend::BackendRuntimeCode::new(state);

			Ok(FetchedCode::FromGenesis { state })
		}
	}

	/// Allocation strategy for pallet benchmarking.
	fn alloc_strategy(heap_pages: Option<u64>) -> HeapAllocStrategy {
		heap_pages.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |p| HeapAllocStrategy::Static {
			extra_pages: p as _,
		})
	}

	fn output(
		&self,
		batches: &[BenchmarkBatchSplitResults],
		storage_info: &[StorageInfo],
		component_ranges: &ComponentRangeMap,
		pov_modes: PovModesMap,
	) -> Result<()> {
		// Jsonify the result and write it to a file or stdout if desired.
		if !self.jsonify(&batches)? && !self.quiet {
			// Print the summary only if `jsonify` did not write to stdout.
			self.print_summary(&batches, &storage_info, pov_modes.clone())
		}

		// Create the weights.rs file.
		if let Some(output_path) = &self.output {
			writer::write_results(
				&batches,
				&storage_info,
				&component_ranges,
				pov_modes,
				self.default_pov_mode,
				output_path,
				self,
			)?;
		}

		Ok(())
	}

	/// Re-analyze a batch historic benchmark timing data. Will not take the PoV into account.
	fn output_from_results(&self, batches: &[BenchmarkBatchSplitResults]) -> Result<()> {
		let mut component_ranges = HashMap::<(String, String), HashMap<String, (u32, u32)>>::new();
		for batch in batches {
			let range = component_ranges
				.entry((
					String::from_utf8(batch.pallet.clone()).unwrap(),
					String::from_utf8(batch.benchmark.clone()).unwrap(),
				))
				.or_default();
			for result in &batch.time_results {
				for (param, value) in &result.components {
					let name = param.to_string();
					let (ref mut min, ref mut max) = range.entry(name).or_insert((*value, *value));
					if *value < *min {
						*min = *value;
					}
					if *value > *max {
						*max = *value;
					}
				}
			}
		}

		let component_ranges: HashMap<_, _> = component_ranges
			.into_iter()
			.map(|(key, ranges)| {
				let ranges = ranges
					.into_iter()
					.map(|(name, (min, max))| ComponentRange { name, min, max })
					.collect();
				(key, ranges)
			})
			.collect();

		self.output(batches, &[], &component_ranges, Default::default())
	}

	/// Jsonifies the passed batches and writes them to stdout or into a file.
	/// Can be configured via `--json` and `--json-file`.
	/// Returns whether it wrote to stdout.
	fn jsonify(&self, batches: &[BenchmarkBatchSplitResults]) -> Result<bool> {
		if self.json_output || self.json_file.is_some() {
			let json = serde_json::to_string_pretty(&batches)
				.map_err(|e| format!("Serializing into JSON: {:?}", e))?;

			if let Some(path) = &self.json_file {
				fs::write(path, json)?;
			} else {
				print!("{json}");
				return Ok(true)
			}
		}

		Ok(false)
	}

	/// Prints the results as human-readable summary without raw timing data.
	fn print_summary(
		&self,
		batches: &[BenchmarkBatchSplitResults],
		storage_info: &[StorageInfo],
		pov_modes: PovModesMap,
	) {
		for batch in batches.iter() {
			// Print benchmark metadata
			let pallet = String::from_utf8(batch.pallet.clone()).expect("Encoded from String; qed");
			let benchmark =
				String::from_utf8(batch.benchmark.clone()).expect("Encoded from String; qed");
			println!(
					"Pallet: {:?}, Extrinsic: {:?}, Lowest values: {:?}, Highest values: {:?}, Steps: {:?}, Repeat: {:?}",
					pallet,
					benchmark,
					self.lowest_range_values,
					self.highest_range_values,
					self.steps,
					self.repeat,
				);

			// Skip raw data + analysis if there are no results
			if batch.time_results.is_empty() {
				continue
			}

			if !self.no_storage_info {
				let mut storage_per_prefix = HashMap::<Vec<u8>, Vec<BenchmarkResult>>::new();
				let pov_mode = pov_modes.get(&(pallet, benchmark)).cloned().unwrap_or_default();

				let comments = writer::process_storage_results(
					&mut storage_per_prefix,
					&batch.db_results,
					storage_info,
					&pov_mode,
					self.default_pov_mode,
					self.worst_case_map_values,
					self.additional_trie_layers,
				);
				println!("Raw Storage Info\n========");
				for comment in comments {
					println!("{}", comment);
				}
				println!();
			}

			// Conduct analysis.
			if !self.no_median_slopes {
				println!("Median Slopes Analysis\n========");
				if let Some(analysis) =
					Analysis::median_slopes(&batch.time_results, BenchmarkSelector::ExtrinsicTime)
				{
					println!("-- Extrinsic Time --\n{}", analysis);
				}
				if let Some(analysis) =
					Analysis::median_slopes(&batch.db_results, BenchmarkSelector::Reads)
				{
					println!("Reads = {:?}", analysis);
				}
				if let Some(analysis) =
					Analysis::median_slopes(&batch.db_results, BenchmarkSelector::Writes)
				{
					println!("Writes = {:?}", analysis);
				}
				if let Some(analysis) =
					Analysis::median_slopes(&batch.db_results, BenchmarkSelector::ProofSize)
				{
					println!("Recorded proof Size = {:?}", analysis);
				}
				println!();
			}
			if !self.no_min_squares {
				println!("Min Squares Analysis\n========");
				if let Some(analysis) =
					Analysis::min_squares_iqr(&batch.time_results, BenchmarkSelector::ExtrinsicTime)
				{
					println!("-- Extrinsic Time --\n{}", analysis);
				}
				if let Some(analysis) =
					Analysis::min_squares_iqr(&batch.db_results, BenchmarkSelector::Reads)
				{
					println!("Reads = {:?}", analysis);
				}
				if let Some(analysis) =
					Analysis::min_squares_iqr(&batch.db_results, BenchmarkSelector::Writes)
				{
					println!("Writes = {:?}", analysis);
				}
				if let Some(analysis) =
					Analysis::min_squares_iqr(&batch.db_results, BenchmarkSelector::ProofSize)
				{
					println!("Recorded proof Size = {:?}", analysis);
				}
				println!();
			}
		}
	}

	/// Parses the PoV modes per benchmark that were specified by the `#[pov_mode]` attribute.
	fn parse_pov_modes(benchmarks: &Vec<SelectedBenchmark>) -> Result<PovModesMap> {
		use std::collections::hash_map::Entry;
		let mut parsed = PovModesMap::new();

		for SelectedBenchmark { pallet, extrinsic, pov_modes, .. } in benchmarks {
			for (pallet_storage, mode) in pov_modes {
				let mode = PovEstimationMode::from_str(&mode)?;
				let splits = pallet_storage.split("::").collect::<Vec<_>>();
				if splits.is_empty() || splits.len() > 2 {
					return Err(format!(
						"Expected 'Pallet::Storage' as storage name but got: {}",
						pallet_storage
					)
					.into())
				}
				let (pov_pallet, pov_storage) = (splits[0], splits.get(1).unwrap_or(&"ALL"));

				match parsed
					.entry((pallet.clone(), extrinsic.clone()))
					.or_default()
					.entry((pov_pallet.to_string(), pov_storage.to_string()))
				{
					Entry::Occupied(_) =>
						return Err(format!(
							"Cannot specify pov_mode tag twice for the same key: {}",
							pallet_storage
						)
						.into()),
					Entry::Vacant(e) => {
						e.insert(mode);
					},
				}
			}
		}
		Ok(parsed)
	}

	/// Sanity check the CLI arguments.
	fn check_args(&self) -> Result<()> {
		if self.runtime.is_some() && self.shared_params.chain.is_some() {
			unreachable!("Clap should not allow both `--runtime` and `--chain` to be provided.")
		}

		if let Some(output_path) = &self.output {
			if !output_path.is_dir() && output_path.file_name().is_none() {
				return Err("Output file or path is invalid!".into())
			}
		}

		if let Some(header_file) = &self.header {
			if !header_file.is_file() {
				return Err("Header file is invalid!".into())
			};
		}

		if let Some(handlebars_template_file) = &self.template {
			if !handlebars_template_file.is_file() {
				return Err("Handlebars template file is invalid!".into())
			};
		}

		Ok(())
	}
}

impl CliConfiguration for PalletCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn chain_id(&self, _is_dev: bool) -> Result<String> {
		Ok(match self.shared_params.chain {
			Some(ref chain) => chain.clone(),
			None => "dev".into(),
		})
	}
}

/// List the benchmarks available in the runtime, in a CSV friendly format.
fn list_benchmark(
	benchmarks_to_run: Vec<SelectedBenchmark>,
	list_output: ListOutput,
	no_csv_header: bool,
) {
	let mut benchmarks = BTreeMap::new();

	// Sort and de-dub by pallet and function name.
	benchmarks_to_run.iter().for_each(|bench| {
		benchmarks
			.entry(&bench.pallet)
			.or_insert_with(BTreeSet::new)
			.insert(&bench.extrinsic);
	});

	match list_output {
		ListOutput::All => {
			if !no_csv_header {
				println!("pallet, extrinsic");
			}
			for (pallet, extrinsics) in benchmarks {
				for extrinsic in extrinsics {
					println!("{pallet}, {extrinsic}");
				}
			}
		},
		ListOutput::Pallets => {
			if !no_csv_header {
				println!("pallet");
			};
			for pallet in benchmarks.keys() {
				println!("{pallet}");
			}
		},
	}
}
