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

//! Interfaces, types and utils for benchmarking a FRAME runtime.
use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchErrorWithPostInfo, pallet_prelude::*, traits::StorageInfo};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_io::hashing::blake2_256;
use sp_runtime::{
	traits::TrailingZeroInput, transaction_validity::TransactionValidityError, DispatchError,
};
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, AllocateAndReturnPointer, PassFatPointerAndDecode,
	PassFatPointerAndRead,
};
use sp_storage::TrackedStorageKey;

/// An alphabet of possible parameters to use for benchmarking.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, Copy, PartialEq, Debug, TypeInfo)]
#[allow(missing_docs)]
#[allow(non_camel_case_types)]
pub enum BenchmarkParameter {
	a,
	b,
	c,
	d,
	e,
	f,
	g,
	h,
	i,
	j,
	k,
	l,
	m,
	n,
	o,
	p,
	q,
	r,
	s,
	t,
	u,
	v,
	w,
	x,
	y,
	z,
}

#[cfg(feature = "std")]
impl std::fmt::Display for BenchmarkParameter {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self)
	}
}

/// The results of a single of benchmark.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo)]
pub struct BenchmarkBatch {
	/// The pallet containing this benchmark.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub pallet: Vec<u8>,
	/// The instance of this pallet being benchmarked.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub instance: Vec<u8>,
	/// The extrinsic (or benchmark name) of this benchmark.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub benchmark: Vec<u8>,
	/// The results from this benchmark.
	pub results: Vec<BenchmarkResult>,
}

// TODO: could probably make API cleaner here.
/// The results of a single of benchmark, where time and db results are separated.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub struct BenchmarkBatchSplitResults {
	/// The pallet containing this benchmark.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub pallet: Vec<u8>,
	/// The instance of this pallet being benchmarked.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub instance: Vec<u8>,
	/// The extrinsic (or benchmark name) of this benchmark.
	#[cfg_attr(feature = "std", serde(with = "serde_as_str"))]
	pub benchmark: Vec<u8>,
	/// The extrinsic timing results from this benchmark.
	pub time_results: Vec<BenchmarkResult>,
	/// The db tracking results from this benchmark.
	pub db_results: Vec<BenchmarkResult>,
}

/// Result from running benchmarks on a FRAME pallet.
/// Contains duration of the function call in nanoseconds along with the benchmark parameters
/// used for that benchmark result.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo)]
pub struct BenchmarkResult {
	pub components: Vec<(BenchmarkParameter, u32)>,
	pub extrinsic_time: u128,
	pub storage_root_time: u128,
	pub reads: u32,
	pub repeat_reads: u32,
	pub writes: u32,
	pub repeat_writes: u32,
	pub proof_size: u32,
	#[cfg_attr(feature = "std", serde(skip))]
	pub keys: Vec<(Vec<u8>, u32, u32, bool)>,
}

impl BenchmarkResult {
	pub fn from_weight(w: Weight) -> Self {
		Self { extrinsic_time: (w.ref_time() / 1_000) as u128, ..Default::default() }
	}
}

/// Helper module to make serde serialize `Vec<u8>` as strings.
#[cfg(feature = "std")]
mod serde_as_str {
	pub fn serialize<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let s = std::str::from_utf8(value).map_err(serde::ser::Error::custom)?;
		serializer.collect_str(s)
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
	where
		D: serde::de::Deserializer<'de>,
	{
		let s: &str = serde::de::Deserialize::deserialize(deserializer)?;
		Ok(s.into())
	}
}

/// Possible errors returned from the benchmarking pipeline.
#[derive(Clone, PartialEq, Debug)]
pub enum BenchmarkError {
	/// The benchmarking pipeline should stop and return the inner string.
	Stop(&'static str),
	/// The benchmarking pipeline is allowed to fail here, and we should use the
	/// included weight instead.
	Override(BenchmarkResult),
	/// The benchmarking pipeline is allowed to fail here, and we should simply
	/// skip processing these results.
	Skip,
	/// No weight can be determined; set the weight of this call to zero.
	///
	/// You can also use `Override` instead, but this is easier to use since `Override` expects the
	/// correct components to be present.
	Weightless,
}

impl From<BenchmarkError> for &'static str {
	fn from(e: BenchmarkError) -> Self {
		match e {
			BenchmarkError::Stop(s) => s,
			BenchmarkError::Override(_) => "benchmark override",
			BenchmarkError::Skip => "benchmark skip",
			BenchmarkError::Weightless => "benchmark weightless",
		}
	}
}

impl From<&'static str> for BenchmarkError {
	fn from(s: &'static str) -> Self {
		Self::Stop(s)
	}
}

impl From<DispatchErrorWithPostInfo> for BenchmarkError {
	fn from(e: DispatchErrorWithPostInfo) -> Self {
		Self::Stop(e.into())
	}
}

impl From<DispatchError> for BenchmarkError {
	fn from(e: DispatchError) -> Self {
		Self::Stop(e.into())
	}
}

impl From<TransactionValidityError> for BenchmarkError {
	fn from(e: TransactionValidityError) -> Self {
		Self::Stop(e.into())
	}
}

/// Configuration used to setup and run runtime benchmarks.
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo)]
pub struct BenchmarkConfig {
	/// The encoded name of the pallet to benchmark.
	pub pallet: Vec<u8>,
	/// The encoded name of the pallet instance to benchmark.
	pub instance: Vec<u8>,
	/// The encoded name of the benchmark/extrinsic to run.
	pub benchmark: Vec<u8>,
	/// The selected component values to use when running the benchmark.
	pub selected_components: Vec<(BenchmarkParameter, u32)>,
	/// Enable an extra benchmark iteration which runs the verification logic for a benchmark.
	pub verify: bool,
	/// Number of times to repeat benchmark within the Wasm environment. (versus in the client)
	pub internal_repeats: u32,
}

/// A list of benchmarks available for a particular pallet and instance.
///
/// All `Vec<u8>` must be valid utf8 strings.
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo)]
pub struct BenchmarkList {
	pub pallet: Vec<u8>,
	pub instance: Vec<u8>,
	pub benchmarks: Vec<BenchmarkMetadata>,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo)]
pub struct BenchmarkMetadata {
	pub name: Vec<u8>,
	pub components: Vec<(BenchmarkParameter, u32, u32)>,
	pub pov_modes: Vec<(Vec<u8>, Vec<u8>)>,
}

sp_api::decl_runtime_apis! {
	/// Runtime api for benchmarking a FRAME runtime.
	#[api_version(2)]
	pub trait Benchmark {
		/// Get the benchmark metadata available for this runtime.
		///
		/// Parameters
		/// - `extra`: Also list benchmarks marked "extra" which would otherwise not be
		///            needed for weight calculation.
		fn benchmark_metadata(extra: bool) -> (Vec<BenchmarkList>, Vec<StorageInfo>);

		/// Dispatch the given benchmark.
		fn dispatch_benchmark(config: BenchmarkConfig) -> Result<Vec<BenchmarkBatch>, alloc::string::String>;
	}
}

/// Get the number of nanoseconds passed since the UNIX epoch
///
/// WARNING! This is a non-deterministic call. Do not use this within
/// consensus critical logic.
pub fn current_time() -> u128 {
	u128::from_le_bytes(self::benchmarking::current_time())
}

/// Interface that provides functions for benchmarking the runtime.
#[sp_runtime_interface::runtime_interface]
pub trait Benchmarking {
	/// Get the number of nanoseconds passed since the UNIX epoch, as u128 le-bytes.
	///
	/// You may want to use the standalone function [`current_time`].
	///
	/// WARNING! This is a non-deterministic call. Do not use this within
	/// consensus critical logic.
	fn current_time() -> AllocateAndReturnPointer<[u8; 16], 16> {
		std::time::SystemTime::now()
			.duration_since(std::time::SystemTime::UNIX_EPOCH)
			.expect("Unix time doesn't go backwards; qed")
			.as_nanos()
			.to_le_bytes()
	}

	/// Reset the trie database to the genesis state.
	fn wipe_db(&mut self) {
		self.wipe()
	}

	/// Commit pending storage changes to the trie database and clear the database cache.
	fn commit_db(&mut self) {
		self.commit()
	}

	/// Get the read/write count.
	fn read_write_count(&self) -> AllocateAndReturnByCodec<(u32, u32, u32, u32)> {
		self.read_write_count()
	}

	/// Reset the read/write count.
	fn reset_read_write_count(&mut self) {
		self.reset_read_write_count()
	}

	/// Get the DB whitelist.
	fn get_whitelist(&self) -> AllocateAndReturnByCodec<Vec<TrackedStorageKey>> {
		self.get_whitelist()
	}

	/// Set the DB whitelist.
	fn set_whitelist(&mut self, new: PassFatPointerAndDecode<Vec<TrackedStorageKey>>) {
		self.set_whitelist(new)
	}

	// Add a new item to the DB whitelist.
	fn add_to_whitelist(&mut self, add: PassFatPointerAndDecode<TrackedStorageKey>) {
		let mut whitelist = self.get_whitelist();

		// Check if add.key is a prefix of any existing key (add covers existing)
		let mut covered_existing = Vec::new();
		for (i, existing) in whitelist.iter().enumerate() {
			if existing.key.starts_with(&add.key) {
				covered_existing.push(i);
			}
		}

		// Remove covered keys and accumulate their reads/writes
		let mut total_reads = add.reads;
		let mut total_writes = add.writes;
		let mut total_whitelisted = add.whitelisted;

		for &i in covered_existing.iter().rev() {
			let existing = whitelist.remove(i);
			total_reads += existing.reads;
			total_writes += existing.writes;
			total_whitelisted = total_whitelisted || existing.whitelisted;
		}

		// Check if any existing key is a prefix of add.key (existing covers add)
		if let Some(existing_prefix) =
			whitelist.iter_mut().find(|existing| add.key.starts_with(&existing.key))
		{
			// Existing prefix covers our new key
			existing_prefix.reads += add.reads;
			existing_prefix.writes += add.writes;
			existing_prefix.whitelisted = existing_prefix.whitelisted || add.whitelisted;
		} else {
			// No existing relationship, add as new
			let new_key = TrackedStorageKey {
				key: add.key,
				reads: total_reads,
				writes: total_writes,
				whitelisted: total_whitelisted,
			};
			whitelist.push(new_key);
		}
		self.set_whitelist(whitelist);
	}

	// Remove an item from the DB whitelist.
	fn remove_from_whitelist(&mut self, remove: PassFatPointerAndRead<Vec<u8>>) {
		let mut whitelist = self.get_whitelist();
		whitelist.retain(|x| x.key != remove);
		self.set_whitelist(whitelist);
	}

	fn get_read_and_written_keys(
		&self,
	) -> AllocateAndReturnByCodec<Vec<(Vec<u8>, u32, u32, bool)>> {
		self.get_read_and_written_keys()
	}

	/// Get current estimated proof size.
	fn proof_size(&self) -> AllocateAndReturnByCodec<Option<u32>> {
		self.proof_size()
	}
}

/// The pallet benchmarking trait.
pub trait Benchmarking {
	/// Get the benchmarks available for this pallet. Generally there is one benchmark per
	/// extrinsic, so these are sometimes just called "extrinsics".
	///
	/// Parameters
	/// - `extra`: Also return benchmarks marked "extra" which would otherwise not be needed for
	///   weight calculation.
	fn benchmarks(extra: bool) -> Vec<BenchmarkMetadata>;

	/// Run the benchmarks for this pallet.
	fn run_benchmark(
		name: &[u8],
		selected_components: &[(BenchmarkParameter, u32)],
		whitelist: &[TrackedStorageKey],
		verify: bool,
		internal_repeats: u32,
	) -> Result<Vec<BenchmarkResult>, BenchmarkError>;
}

/// The recording trait used to mark the start and end of a benchmark.
pub trait Recording {
	/// Start the benchmark.
	fn start(&mut self) {}

	// Stop the benchmark.
	fn stop(&mut self) {}
}

/// A no-op recording, used for unit test.
struct NoopRecording;
impl Recording for NoopRecording {}

/// A no-op recording, used for tests that should setup some state before running the benchmark.
struct TestRecording<'a> {
	on_before_start: Option<&'a dyn Fn()>,
}

impl<'a> TestRecording<'a> {
	fn new(on_before_start: &'a dyn Fn()) -> Self {
		Self { on_before_start: Some(on_before_start) }
	}
}

impl<'a> Recording for TestRecording<'a> {
	fn start(&mut self) {
		(self.on_before_start.take().expect("start called more than once"))();
	}
}

/// Records the time and proof size of a single benchmark iteration.
pub struct BenchmarkRecording<'a> {
	on_before_start: Option<&'a dyn Fn()>,
	start_extrinsic: Option<u128>,
	finish_extrinsic: Option<u128>,
	start_pov: Option<u32>,
	end_pov: Option<u32>,
}

impl<'a> BenchmarkRecording<'a> {
	pub fn new(on_before_start: &'a dyn Fn()) -> Self {
		Self {
			on_before_start: Some(on_before_start),
			start_extrinsic: None,
			finish_extrinsic: None,
			start_pov: None,
			end_pov: None,
		}
	}
}

impl<'a> Recording for BenchmarkRecording<'a> {
	fn start(&mut self) {
		(self.on_before_start.take().expect("start called more than once"))();
		self.start_pov = crate::benchmarking::proof_size();
		self.start_extrinsic = Some(current_time());
	}

	fn stop(&mut self) {
		self.finish_extrinsic = Some(current_time());
		self.end_pov = crate::benchmarking::proof_size();
	}
}

impl<'a> BenchmarkRecording<'a> {
	pub fn start_pov(&self) -> Option<u32> {
		self.start_pov
	}

	pub fn end_pov(&self) -> Option<u32> {
		self.end_pov
	}

	pub fn diff_pov(&self) -> Option<u32> {
		self.start_pov.zip(self.end_pov).map(|(start, end)| end.saturating_sub(start))
	}

	pub fn elapsed_extrinsic(&self) -> Option<u128> {
		self.start_extrinsic
			.zip(self.finish_extrinsic)
			.map(|(start, end)| end.saturating_sub(start))
	}
}

/// The required setup for creating a benchmark.
///
/// Instance generic parameter is optional and can be used in order to capture unused generics for
/// instantiable pallets.
pub trait BenchmarkingSetup<T, I = ()> {
	/// Return the components and their ranges which should be tested in this benchmark.
	fn components(&self) -> Vec<(BenchmarkParameter, u32, u32)>;

	/// Set up the storage, and prepare a closure to run the benchmark.
	fn instance(
		&self,
		recording: &mut impl Recording,
		components: &[(BenchmarkParameter, u32)],
		verify: bool,
	) -> Result<(), BenchmarkError>;

	/// Same as `instance` but passing a closure to run before the benchmark starts.
	fn test_instance(
		&self,
		components: &[(BenchmarkParameter, u32)],
		on_before_start: &dyn Fn(),
	) -> Result<(), BenchmarkError> {
		return self.instance(&mut TestRecording::new(on_before_start), components, true);
	}

	/// Same as `instance` but passing a no-op recording for unit tests.
	fn unit_test_instance(
		&self,
		components: &[(BenchmarkParameter, u32)],
	) -> Result<(), BenchmarkError> {
		return self.instance(&mut NoopRecording {}, components, true);
	}
}

/// Grab an account, seeded by a name and index.
pub fn account<AccountId: Decode>(name: &'static str, index: u32, seed: u32) -> AccountId {
	let entropy = (name, index, seed).using_encoded(blake2_256);
	Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
		.expect("infinite length input; no invalid inputs for type; qed")
}

/// This caller account is automatically whitelisted for DB reads/writes by the benchmarking macro.
pub fn whitelisted_caller<AccountId: Decode>() -> AccountId {
	account::<AccountId>("whitelisted_caller", 0, 0)
}

#[macro_export]
macro_rules! whitelist_account {
	($acc:ident) => {
		frame_benchmarking::benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&$acc).into(),
		);
	};
}

#[cfg(test)]
mod tests {
	use sc_client_db::BenchmarkingState;
	use sp_core::storage::TrackedStorageKey;
	use sp_runtime::traits::BlakeTwo256;

	#[test]
	fn test_add_to_whitelist_prefix_handling() {
		let state =
			BenchmarkingState::<BlakeTwo256>::new(Default::default(), None, false, true).unwrap();

		let mut overlay = Default::default();
		let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);

		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			// Add a prefix first
			let prefix_key = TrackedStorageKey {
				key: b"System::Account".to_vec(),
				reads: 1,
				writes: 0,
				whitelisted: true,
			};
			crate::benchmarking::add_to_whitelist(prefix_key.clone());

			// Now add a key that starts with the prefix
			let specific_key = TrackedStorageKey {
				key: b"System::Account::12345".to_vec(),
				reads: 2,
				writes: 1,
				whitelisted: true,
			};
			crate::benchmarking::add_to_whitelist(specific_key);

			// The prefix should now have combined reads/writes
			let whitelist = crate::benchmarking::get_whitelist();
			assert_eq!(whitelist.len(), 1);
			assert_eq!(whitelist[0].key, b"System::Account".to_vec());
			assert_eq!(whitelist[0].reads, 3); // 1 + 2
			assert_eq!(whitelist[0].writes, 1); // 0 + 1
			assert!(whitelist[0].whitelisted);
		});
	}

	#[test]
	fn test_add_prefix_that_covers_existing_specific_keys() {
		let state =
			BenchmarkingState::<BlakeTwo256>::new(Default::default(), None, false, true).unwrap();

		let mut overlay = Default::default();
		let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);

		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			// Add specific keys first
			for i in 0..3 {
				let specific_key = TrackedStorageKey {
					key: format!("System::Account::{}", i).into_bytes(),
					reads: 1,
					writes: 1,
					whitelisted: true,
				};
				crate::benchmarking::add_to_whitelist(specific_key);
			}

			// Initial whitelist should have 3 specific keys
			let whitelist = crate::benchmarking::get_whitelist();
			assert_eq!(whitelist.len(), 3);

			// Now add a prefix that covers all of them
			let prefix_key = TrackedStorageKey {
				key: b"System::Account".to_vec(),
				reads: 5,
				writes: 2,
				whitelisted: true,
			};
			crate::benchmarking::add_to_whitelist(prefix_key);

			// The prefix should absorb all specific keys and have combined reads/writes
			let whitelist = crate::benchmarking::get_whitelist();
			assert_eq!(whitelist.len(), 1);
			assert_eq!(whitelist[0].key, b"System::Account".to_vec());
			assert_eq!(whitelist[0].reads, 8); // 5 + 1 + 1 + 1
			assert_eq!(whitelist[0].writes, 5); // 2 + 1 + 1 + 1
			assert!(whitelist[0].whitelisted);
		});
	}

	#[test]
	fn test_unrelated_keys() {
		let state =
			BenchmarkingState::<BlakeTwo256>::new(Default::default(), None, false, true).unwrap();

		let mut overlay = Default::default();
		let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);

		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			let key1 = TrackedStorageKey {
				key: b"System::Account".to_vec(),
				reads: 1,
				writes: 0,
				whitelisted: true,
			};

			let key2 = TrackedStorageKey {
				key: b"Timestamp::Now".to_vec(),
				reads: 2,
				writes: 1,
				whitelisted: true,
			};

			crate::benchmarking::add_to_whitelist(key1);
			crate::benchmarking::add_to_whitelist(key2);

			// Both should remain
			let whitelist = crate::benchmarking::get_whitelist();
			assert_eq!(whitelist.len(), 2);

			// Verify both keys exist
			assert!(whitelist.iter().any(|k| k.key == b"System::Account".to_vec()));
			assert!(whitelist.iter().any(|k| k.key == b"Timestamp::Now".to_vec()));
		});
	}

	#[test]
	fn test_multiple_specifics_to_same_prefix() {
		let state =
			BenchmarkingState::<BlakeTwo256>::new(Default::default(), None, false, true).unwrap();

		let mut overlay = Default::default();
		let mut ext = sp_state_machine::Ext::new(&mut overlay, &state, None);

		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			// Add prefix first
			let prefix_key = TrackedStorageKey {
				key: b"System::Account".to_vec(),
				reads: 5,
				writes: 2,
				whitelisted: true,
			};
			crate::benchmarking::add_to_whitelist(prefix_key);

			// Add multiple specific keys
			for i in 0..5 {
				let specific_key = TrackedStorageKey {
					key: format!("System::Account::user{}", i).into_bytes(),
					reads: 1,
					writes: 1,
					whitelisted: true,
				};
				crate::benchmarking::add_to_whitelist(specific_key);
			}

			// Should still have only the prefix with accumulated reads/writes
			let whitelist = crate::benchmarking::get_whitelist();
			assert_eq!(whitelist.len(), 1);
			assert_eq!(whitelist[0].key, b"System::Account".to_vec());
			assert_eq!(whitelist[0].reads, 10); // 5 + 1*5
			assert_eq!(whitelist[0].writes, 7); // 2 + 1*5
		});
	}
}
