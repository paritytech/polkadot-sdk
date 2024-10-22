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
use crate::pallet::{types::FetchedCode, GenesisBuilderPolicy};
use codec::{Decode, Encode};
use frame_benchmarking::{
	Analysis, BenchmarkBatch, BenchmarkBatchSplitResults, BenchmarkList, BenchmarkParameter,
	BenchmarkResult, BenchmarkSelector,
};
use frame_support::traits::StorageInfo;
use linked_hash_map::LinkedHashMap;
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sc_cli::{execution_method_from_cli, ChainSpec, CliConfiguration, Result, SharedParams};
use sc_client_db::BenchmarkingState;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sp_core::{
	offchain::{
		testing::{TestOffchainExt, TestTransactionPoolExt},
		OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
	},
	traits::{CallContext, CodeExecutor, ReadRuntimeVersionExt, WrappedRuntimeCode},
	Hasher,
};
use sp_externalities::Extensions;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::traits::Hash;
use sp_state_machine::StateMachine;
use sp_storage::{well_known_keys::CODE, Storage};
use sp_trie::{proof_size_extension::ProofSizeExt, recorder::Recorder};
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

		let genesis_storage = self.genesis_storage::<ExtraHostFunctions>(&chain_spec)?;

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
			// Proof recording depends on CLI settings
			!self.disable_proof_recording,
			// Do not enable storage tracking
			false,
		)?;

		let method =
			execution_method_from_cli(self.wasm_method, self.wasmtime_instantiation_strategy);

		let state = &state_without_tracking;
		let runtime = self.runtime_blob(&state_without_tracking)?;
		let runtime_code = runtime.code()?;
		let alloc_strategy = self.alloc_strategy(runtime_code.heap_pages);

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
					&mut Default::default(),
					&executor,
					"Benchmark_benchmark_metadata",
					&(self.extra).encode(),
					&mut Self::build_extensions(executor.clone(), state.recorder()),
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
					let state = &state_without_tracking;
					// Don't use these results since verification code will add overhead.
					let _batch: Vec<BenchmarkBatch> = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state,
							&mut Default::default(),
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
							&mut Self::build_extensions(executor.clone(), state.recorder()),
							&runtime_code,
							CallContext::Offchain,
						),
						"dispatch a benchmark",
					) {
						Err(e) => {
							log::error!(target: LOG_TARGET, "Error executing and verifying runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Err(e)) => {
							log::error!(target: LOG_TARGET, "Error executing and verifying runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Ok(b)) => b,
					};
				}
				// Do one loop of DB tracking.
				{
					let state = &state_with_tracking;
					let batch: Vec<BenchmarkBatch> = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state, // todo remove tracking
							&mut Default::default(),
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
							&mut Self::build_extensions(executor.clone(), state.recorder()),
							&runtime_code,
							CallContext::Offchain,
						),
						"dispatch a benchmark",
					) {
						Err(e) => {
							log::error!(target: LOG_TARGET, "Error executing runtime benchmark: {}", e);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Err(e)) => {
							log::error!(target: LOG_TARGET, "Benchmark {pallet}::{extrinsic} failed: {e}",);
							failed.push((pallet.clone(), extrinsic.clone()));
							continue 'outer
						},
						Ok(Ok(b)) => b,
					};

					batches_db.extend(batch);
				}
				// Finally run a bunch of loops to get extrinsic timing information.
				for r in 0..self.external_repeat {
					let state = &state_without_tracking;
					let batch = match Self::exec_state_machine::<
						std::result::Result<Vec<BenchmarkBatch>, String>,
						_,
						_,
					>(
						StateMachine::new(
							state, // todo remove tracking
							&mut Default::default(),
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
							&mut Self::build_extensions(executor.clone(), state.recorder()),
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
		let extrinsic = self.extrinsic.clone().unwrap_or_default();
		let extrinsic_split: Vec<&str> = extrinsic.split(',').collect();
		let extrinsics: Vec<_> = extrinsic_split.iter().map(|x| x.trim().as_bytes()).collect();

		// Use the benchmark list and the user input to determine the set of benchmarks to run.
		let mut benchmarks_to_run = Vec::new();
		list.iter().filter(|item| self.pallet_selected(&item.pallet)).for_each(|item| {
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

	/// Whether this pallet should be run.
	fn pallet_selected(&self, pallet: &Vec<u8>) -> bool {
		let include = self.pallet.clone().unwrap_or_default();

		let included = include.is_empty() || include == "*" || include.as_bytes() == pallet;
		let excluded = self.exclude_pallets.iter().any(|p| p.as_bytes() == pallet);

		included && !excluded
	}

	/// Build the genesis storage by either the Genesis Builder API, chain spec or nothing.
	///
	/// Behaviour can be controlled by the `--genesis-builder` flag.
	fn genesis_storage<F: HostFunctions>(
		&self,
		chain_spec: &Option<Box<dyn ChainSpec>>,
	) -> Result<sp_storage::Storage> {
		Ok(match (self.genesis_builder, self.runtime.as_ref()) {
			(Some(GenesisBuilderPolicy::None), _) => Storage::default(),
			(Some(GenesisBuilderPolicy::SpecGenesis | GenesisBuilderPolicy::Spec), Some(_)) =>
					return Err("Cannot use `--genesis-builder=spec-genesis` with `--runtime` since the runtime would be ignored.".into()),
			(Some(GenesisBuilderPolicy::SpecGenesis | GenesisBuilderPolicy::Spec), None) | (None, None) => {
				log::warn!(target: LOG_TARGET, "{WARN_SPEC_GENESIS_CTOR}");
				let Some(chain_spec) = chain_spec else {
					return Err("No chain spec specified to generate the genesis state".into());
				};

				let storage = chain_spec
					.build_storage()
					.map_err(|e| format!("{ERROR_CANNOT_BUILD_GENESIS}\nError: {e}"))?;

				storage
			},
			(Some(GenesisBuilderPolicy::SpecRuntime), Some(_)) =>
				return Err("Cannot use `--genesis-builder=spec` with `--runtime` since the runtime would be ignored.".into()),
			(Some(GenesisBuilderPolicy::SpecRuntime), None) => {
				let Some(chain_spec) = chain_spec else {
					return Err("No chain spec specified to generate the genesis state".into());
				};

				self.genesis_from_spec_runtime::<F>(chain_spec.as_ref())?
			},
			(Some(GenesisBuilderPolicy::Runtime), None) => return Err("Cannot use `--genesis-builder=runtime` without `--runtime`".into()),
			(Some(GenesisBuilderPolicy::Runtime), Some(runtime)) | (None, Some(runtime)) => {
				log::info!(target: LOG_TARGET, "Loading WASM from {}", runtime.display());

				let code = fs::read(&runtime).map_err(|e| {
					format!(
						"Could not load runtime file from path: {}, error: {}",
						runtime.display(),
						e
					)
				})?;

				self.genesis_from_code::<F>(&code)?
			}
		})
	}

	/// Setup the genesis state by calling the runtime APIs of the chain-specs genesis runtime.
	fn genesis_from_spec_runtime<EHF: HostFunctions>(
		&self,
		chain_spec: &dyn ChainSpec,
	) -> Result<Storage> {
		log::info!(target: LOG_TARGET, "Building genesis state from chain spec runtime");
		let storage = chain_spec
			.build_storage()
			.map_err(|e| format!("{ERROR_CANNOT_BUILD_GENESIS}\nError: {e}"))?;

		let code: &Vec<u8> =
			storage.top.get(CODE).ok_or("No runtime code in the genesis storage")?;

		self.genesis_from_code::<EHF>(code)
	}

	fn genesis_from_code<EHF: HostFunctions>(&self, code: &[u8]) -> Result<Storage> {
		let genesis_config_caller = GenesisConfigBuilderRuntimeCaller::<(
			sp_io::SubstrateHostFunctions,
			frame_benchmarking::benchmarking::HostFunctions,
			EHF,
		)>::new(code);
		let preset = Some(&self.genesis_builder_preset);

		let mut storage =
			genesis_config_caller.get_storage_for_named_preset(preset).inspect_err(|e| {
				let presets = genesis_config_caller.preset_names().unwrap_or_default();
				log::error!(
					target: LOG_TARGET,
					"Please pick one of the available presets with \
			`--genesis-builder-preset=<PRESET>` or use a different `--genesis-builder-policy`. Available presets ({}): {:?}. Error: {:?}",
					presets.len(),
					presets,
					e
				);
			})?;

		storage.top.insert(CODE.into(), code.into());

		Ok(storage)
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
	fn build_extensions<E: CodeExecutor, H: Hasher + 'static>(
		exe: E,
		maybe_recorder: Option<Recorder<H>>,
	) -> Extensions {
		let mut extensions = Extensions::default();
		let (offchain, _) = TestOffchainExt::new();
		let (pool, _) = TestTransactionPoolExt::new();
		let keystore = MemoryKeystore::new();
		extensions.register(KeystoreExt::new(keystore));
		extensions.register(OffchainWorkerExt::new(offchain.clone()));
		extensions.register(OffchainDbExt::new(offchain));
		extensions.register(TransactionPoolExt::new(pool));
		extensions.register(ReadRuntimeVersionExt::new(exe));
		if let Some(recorder) = maybe_recorder {
			extensions.register(ProofSizeExt::new(recorder));
		}
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
		if let Some(runtime) = self.runtime.as_ref() {
			log::info!(target: LOG_TARGET, "Loading WASM from file");
			let code = fs::read(runtime).map_err(|e| {
				format!(
					"Could not load runtime file from path: {}, error: {}",
					runtime.display(),
					e
				)
			})?;
			let hash = sp_core::blake2_256(&code).to_vec();
			let wrapped_code = WrappedRuntimeCode(Cow::Owned(code));

			Ok(FetchedCode::FromFile { wrapped_code, heap_pages: self.heap_pages, hash })
		} else {
			log::info!(target: LOG_TARGET, "Loading WASM from state");
			let state = sp_state_machine::backend::BackendRuntimeCode::new(state);

			Ok(FetchedCode::FromGenesis { state })
		}
	}

	/// Allocation strategy for pallet benchmarking.
	fn alloc_strategy(&self, runtime_heap_pages: Option<u64>) -> HeapAllocStrategy {
		self.heap_pages.or(runtime_heap_pages).map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |p| {
			HeapAllocStrategy::Static { extra_pages: p as _ }
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
				return Err(format!(
					"Output path is neither a directory nor a file: {output_path:?}"
				)
				.into())
			}
		}

		if let Some(header_file) = &self.header {
			if !header_file.is_file() {
				return Err(format!("Header file could not be found: {header_file:?}").into())
			};
		}

		if let Some(handlebars_template_file) = &self.template {
			if !handlebars_template_file.is_file() {
				return Err(format!(
					"Handlebars template file could not be found: {handlebars_template_file:?}"
				)
				.into())
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
