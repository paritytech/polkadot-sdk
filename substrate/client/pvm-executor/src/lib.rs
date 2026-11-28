// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Executor for JAM PolkaVM runtime blobs.
//!
//! [`PvmExecutor`] only runs runtimes built for JAM (`--cfg jam`). The runtime gets the host calls
//! of `J` plus the `grow_heap` and `fetch` imports of `jam-pvm-common`.

#![warn(missing_docs)]

mod runtime;

use codec::Decode;
use parking_lot::Mutex;
use runtime::Runtime;
use sc_executor_common::error::{Error, Result, WasmError};
use schnellru::{ByLength, LruMap};
use sp_core::traits::{CallContext, CodeExecutor, Externalities, FetchRuntimeCode, RuntimeCode};
use sp_version::RuntimeVersion;
use std::{
	marker::PhantomData,
	panic::{AssertUnwindSafe, UnwindSafe},
	sync::Arc,
};

pub use sc_executor_common::RuntimeVersionOf;

const LOG_TARGET: &str = "pvm-executor";

/// Run `f` with `ext` set as the externalities, catching any panic.
fn with_externalities_safe<F, U>(ext: &mut dyn Externalities, f: F) -> Result<U>
where
	F: UnwindSafe + FnOnce() -> U,
{
	sp_externalities::set_and_run_with_externalities(ext, move || {
		// Substrate uses a panic hook that terminates the process; disable it for the call.
		let _guard = sp_panic_handler::AbortGuard::force_unwind();
		std::panic::catch_unwind(f).map_err(|e| {
			if let Some(err) = e.downcast_ref::<String>() {
				Error::RuntimePanicked(err.clone())
			} else if let Some(err) = e.downcast_ref::<&'static str>() {
				Error::RuntimePanicked(err.to_string())
			} else {
				Error::RuntimePanicked("Unknown panic".into())
			}
		})
	})
}

/// A cached runtime together with its version.
struct CachedRuntime {
	runtime: Runtime,
	version: RuntimeVersion,
}

/// Builder for [`PvmExecutor`].
pub struct PvmExecutorBuilder<J> {
	runtime_cache_size: u8,
	_phantom: PhantomData<fn() -> J>,
}

impl<J> PvmExecutorBuilder<J> {
	/// Create a new builder with the default settings.
	pub fn new() -> Self {
		Self { runtime_cache_size: 4, _phantom: PhantomData }
	}

	/// The number of different runtimes the cache keeps compiled, at least `1`.
	///
	/// By default this value is set to `4`.
	pub fn with_runtime_cache_size(mut self, runtime_cache_size: u8) -> Self {
		self.runtime_cache_size = runtime_cache_size;
		self
	}

	/// Build the configured [`PvmExecutor`].
	pub fn build(self) -> PvmExecutor<J> {
		PvmExecutor {
			cache: Arc::new(Mutex::new(LruMap::new(ByLength::new(
				self.runtime_cache_size.max(1) as u32
			)))),
			_phantom: PhantomData,
		}
	}
}

impl<J> Default for PvmExecutorBuilder<J> {
	fn default() -> Self {
		Self::new()
	}
}

/// Executor for JAM PolkaVM runtime blobs, exposing the host calls `J` to the runtime.
///
/// Compiled runtimes are cached by their code hash. Every call runs in a fresh instance.
pub struct PvmExecutor<J> {
	cache: Arc<Mutex<LruMap<Vec<u8>, Arc<CachedRuntime>>>>,
	_phantom: PhantomData<fn() -> J>,
}

impl<J> Clone for PvmExecutor<J> {
	fn clone(&self) -> Self {
		Self { cache: self.cache.clone(), _phantom: PhantomData }
	}
}

impl<J> Default for PvmExecutor<J> {
	fn default() -> Self {
		PvmExecutorBuilder::new().build()
	}
}

impl<J> PvmExecutor<J> {
	/// Instantiate a builder for creating an instance of `Self`.
	pub fn builder() -> PvmExecutorBuilder<J> {
		PvmExecutorBuilder::new()
	}
}

impl<J: sp_jam_runtime_interface::HostFunctions> PvmExecutor<J> {
	/// Returns the runtime for `runtime_code`, compiling it on a cache miss.
	fn runtime(
		&self,
		ext: &mut dyn Externalities,
		runtime_code: &RuntimeCode,
	) -> Result<Arc<CachedRuntime>> {
		if let Some(runtime) = self.cache.lock().get(&runtime_code.hash) {
			return Ok(runtime.clone());
		}

		let code = runtime_code.fetch_runtime_code().ok_or(WasmError::CodeNotFound)?;
		let runtime = Runtime::new(&code, J::host_functions())?;

		// TODO: Read the version from the blob's custom sections instead of executing the runtime.
		let version =
			with_externalities_safe(ext, AssertUnwindSafe(|| runtime.call("Core_version", &[])))??;
		let version = RuntimeVersion::decode(&mut &version[..]).map_err(|_| {
			WasmError::Instantiation("failed to decode the \"Core_version\" result".into())
		})?;

		let runtime = Arc::new(CachedRuntime { runtime, version });
		self.cache.lock().insert(runtime_code.hash.clone(), runtime.clone());
		Ok(runtime)
	}
}

impl<J> sp_core::traits::ReadRuntimeVersion for PvmExecutor<J> {
	/// Not supported: JAM runtimes have no host call that reads the version of a runtime blob.
	fn read_runtime_version(
		&self,
		_code: &[u8],
		_ext: &mut dyn Externalities,
	) -> std::result::Result<Vec<u8>, String> {
		unimplemented!("JAM runtimes never read the runtime version of a runtime blob")
	}
}

impl<J: sp_jam_runtime_interface::HostFunctions + 'static> CodeExecutor for PvmExecutor<J> {
	type Error = Error;

	fn call(
		&self,
		ext: &mut dyn Externalities,
		runtime_code: &RuntimeCode,
		method: &str,
		data: &[u8],
		_context: CallContext,
	) -> (Result<Vec<u8>>, bool) {
		log::trace!(target: LOG_TARGET, "Executing function: {method}");

		let result = self.runtime(ext, runtime_code).and_then(|runtime| {
			with_externalities_safe(ext, AssertUnwindSafe(|| runtime.runtime.call(method, data)))?
		});

		(result, false)
	}
}

impl<J: sp_jam_runtime_interface::HostFunctions> RuntimeVersionOf for PvmExecutor<J> {
	fn runtime_version(
		&self,
		ext: &mut dyn Externalities,
		runtime_code: &RuntimeCode,
	) -> Result<RuntimeVersion> {
		self.runtime(ext, runtime_code).map(|runtime| runtime.version.clone())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_core::traits::WrappedRuntimeCode;

	#[test]
	fn rejects_a_wasm_blob() {
		let code = WrappedRuntimeCode(b"\0asm\x01\0\0\0".as_slice().into());
		let runtime_code = RuntimeCode { code_fetcher: &code, heap_pages: None, hash: vec![1] };
		let mut ext = sp_state_machine::BasicExternalities::default();

		let (result, _) = PvmExecutor::<()>::default().call(
			&mut ext,
			&runtime_code,
			"Core_version",
			&[],
			CallContext::Offchain,
		);

		assert!(matches!(result, Err(Error::RuntimeConstruction(_))), "{result:?}");
	}

	/// Path to a JAM runtime blob, for example the riscv `--cfg jam` build of
	/// `parachain-template-runtime`.
	const RUNTIME_BLOB_ENV: &str = "PVM_EXECUTOR_TEST_RUNTIME";

	type Executor = PvmExecutor<sp_io::jam::HostFunctions>;

	fn runtime_blob() -> Vec<u8> {
		sp_tracing::try_init_simple();
		let path = std::env::var(RUNTIME_BLOB_ENV)
			.unwrap_or_else(|_| panic!("set `{RUNTIME_BLOB_ENV}` to a JAM runtime blob"));
		let blob = std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
		assert!(blob.starts_with(b"PVM\0"), "{path} is not a PolkaVM blob");
		blob
	}

	#[test]
	#[ignore = "needs a JAM runtime blob in `PVM_EXECUTOR_TEST_RUNTIME`"]
	fn runtime_version_is_cached() {
		let blob = runtime_blob();
		let code = WrappedRuntimeCode(blob.as_slice().into());
		let runtime_code = RuntimeCode { code_fetcher: &code, heap_pages: None, hash: vec![1] };
		let mut ext = sp_state_machine::BasicExternalities::default();
		let executor = Executor::default();

		let version = executor.runtime_version(&mut ext, &runtime_code).unwrap();
		assert!(!version.spec_name.is_empty());

		let cached = executor.cache.lock().get(&runtime_code.hash).cloned().unwrap();
		assert_eq!(executor.runtime_version(&mut ext, &runtime_code).unwrap(), version);
		let cached_again = executor.cache.lock().get(&runtime_code.hash).cloned().unwrap();
		assert!(Arc::ptr_eq(&cached, &cached_again));
		assert_eq!(executor.cache.lock().len(), 1);

		let (result, _) =
			executor.call(&mut ext, &runtime_code, "Core_version", &[], CallContext::Offchain);
		assert_eq!(RuntimeVersion::decode(&mut &result.unwrap()[..]).unwrap(), version);
	}

	#[test]
	#[ignore = "needs a JAM runtime blob in `PVM_EXECUTOR_TEST_RUNTIME`"]
	fn build_genesis_state_writes_storage() {
		let blob = runtime_blob();
		let code = WrappedRuntimeCode(blob.as_slice().into());
		let runtime_code = RuntimeCode { code_fetcher: &code, heap_pages: None, hash: vec![1] };
		let mut ext = sp_state_machine::BasicExternalities::default();
		let executor = Executor::default();
		let mut call = |method: &str, data: &[u8]| {
			let (result, _) =
				executor.call(&mut ext, &runtime_code, method, data, CallContext::Offchain);
			result.unwrap()
		};

		let preset = call("GenesisBuilder_get_preset", &None::<String>.encode());
		let preset = Option::<Vec<u8>>::decode(&mut &preset[..]).unwrap().unwrap();
		let result = call("GenesisBuilder_build_state", &preset.encode());
		std::result::Result::<(), String>::decode(&mut &result[..]).unwrap().unwrap();

		let parent_hash_key =
			[sp_crypto_hashing::twox_128(b"System"), sp_crypto_hashing::twox_128(b"ParentHash")]
				.concat();
		assert_eq!(ext.storage(&parent_hash_key), Some(vec![69u8; 32]));
	}
}
