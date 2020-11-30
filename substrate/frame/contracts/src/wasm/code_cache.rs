// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

//! A module that implements instrumented code cache.
//!
//! - In order to run contract code we need to instrument it with gas metering.
//! To do that we need to provide the schedule which will supply exact gas costs values.
//! We cache this code in the storage saving the schedule version.
//! - Before running contract code we check if the cached code has the schedule version that
//! is equal to the current saved schedule.
//! If it is equal then run the code, if it isn't reinstrument with the current schedule.
//! - When we update the schedule we want it to have strictly greater version than the current saved one:
//! this guarantees that every instrumented contract code in cache cannot have the version equal to the current one.
//! Thus, before executing a contract it should be reinstrument with new schedule.

use crate::wasm::{prepare, runtime::Env, PrefabWasmModule};
use crate::{CodeHash, CodeStorage, PristineCode, Schedule, Config};
use sp_std::prelude::*;
use sp_runtime::traits::Hash;
use sp_core::crypto::UncheckedFrom;
use frame_support::StorageMap;

/// Put code in the storage. The hash of code is used as a key and is returned
/// as a result of this function.
///
/// This function instruments the given code and caches it in the storage.
pub fn save<T: Config>(
	original_code: Vec<u8>,
	schedule: &Schedule<T>,
) -> Result<CodeHash<T>, &'static str> where T::AccountId: UncheckedFrom<T::Hash> + AsRef<[u8]> {
	let prefab_module = prepare::prepare_contract::<Env, T>(&original_code, schedule)?;
	let code_hash = T::Hashing::hash(&original_code);

	<CodeStorage<T>>::insert(code_hash, prefab_module);
	<PristineCode<T>>::insert(code_hash, original_code);

	Ok(code_hash)
}

/// Version of `save` to be used in runtime benchmarks.
//
/// This version neither checks nor instruments the passed in code. This is useful
/// when code needs to be benchmarked without the injected instrumentation.
#[cfg(feature = "runtime-benchmarks")]
pub fn save_raw<T: Config>(
	original_code: Vec<u8>,
	schedule: &Schedule<T>,
) -> Result<CodeHash<T>, &'static str> where T::AccountId: UncheckedFrom<T::Hash> + AsRef<[u8]> {
	let prefab_module = prepare::benchmarking::prepare_contract::<T>(&original_code, schedule)?;
	let code_hash = T::Hashing::hash(&original_code);

	<CodeStorage<T>>::insert(code_hash, prefab_module);
	<PristineCode<T>>::insert(code_hash, original_code);

	Ok(code_hash)
}

/// Load code with the given code hash.
///
/// If the module was instrumented with a lower version of schedule than
/// the current one given as an argument, then this function will perform
/// re-instrumentation and update the cache in the storage.
pub fn load<T: Config>(
	code_hash: &CodeHash<T>,
	schedule: &Schedule<T>,
) -> Result<PrefabWasmModule, &'static str> where T::AccountId: UncheckedFrom<T::Hash> + AsRef<[u8]> {
	let mut prefab_module =
		<CodeStorage<T>>::get(code_hash).ok_or_else(|| "code is not found")?;

	if prefab_module.schedule_version < schedule.version {
		// The current schedule version is greater than the version of the one cached
		// in the storage.
		//
		// We need to re-instrument the code with the latest schedule here.
		let original_code =
			<PristineCode<T>>::get(code_hash).ok_or_else(|| "pristine code is not found")?;
		prefab_module = prepare::prepare_contract::<Env, T>(&original_code, schedule)?;
		<CodeStorage<T>>::insert(&code_hash, &prefab_module);
	}
	Ok(prefab_module)
}
