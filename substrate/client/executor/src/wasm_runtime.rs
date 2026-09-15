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

//! Traits and accessor functions for calling into the Substrate Wasm runtime.
//!
//! The primary means of accessing the runtimes is through a cache which saves the reusable
//! components of the runtime that are expensive to initialize.

use crate::error::{Error, WasmError};

use codec::Decode;
use parking_lot::Mutex;
use sc_executor_common::{
	runtime_blob::RuntimeBlob,
	wasm_runtime::{
		HeapAllocStrategy, TimedWasmInstance, TimedWasmModule, WasmInstance, WasmModule,
	},
};
use schnellru::{ByLength, LruMap};
use sp_core::traits::{Externalities, FetchRuntimeCode, RuntimeCode};
use sp_version::RuntimeVersion;
use sp_wasm_interface::HostFunctions;

use std::{
	panic::AssertUnwindSafe,
	path::{Path, PathBuf},
	sync::{Arc, OnceLock},
};

/// Specification of different methods of executing the runtime Wasm code.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum WasmExecutionMethod {
	/// Uses the Wasmtime compiled runtime.
	Compiled {
		/// The instantiation strategy to use.
		instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy,
	},
}

impl Default for WasmExecutionMethod {
	fn default() -> Self {
		Self::Compiled {
			instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
		}
	}
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct VersionedRuntimeId {
	/// Runtime code hash.
	code_hash: Vec<u8>,
	/// Wasm runtime type.
	wasm_method: WasmExecutionMethod,
}

/// Timed counterpart of a cached runtime's module.
///
/// Filled synchronously for engines whose modules are timed-callable as-is (PolkaVM — same
/// compile, uncapped), by a background compile thread for wasmtime (separate epoch-interruption
/// compile). Always eventually set, so [`OnceLock::wait`] cannot hang.
type TimedModuleSlot = Arc<OnceLock<Result<Box<dyn TimedWasmModule>, WasmError>>>;

/// A Wasm runtime object along with its cached runtime version.
struct VersionedRuntime {
	/// Shared runtime that can spawn instances.
	module: Box<dyn WasmModule>,
	/// Timed counterpart of the module.
	timed_module: TimedModuleSlot,
	/// Runtime version according to `Core_version` if any.
	version: Option<RuntimeVersion>,

	// TODO: Remove this once the legacy instance reuse instantiation strategy
	//       for `wasmtime` is gone, as this only makes sense with that particular strategy.
	/// Cached instance pool.
	instances: Vec<Mutex<Option<Box<dyn WasmInstance>>>>,
}

impl VersionedRuntime {
	/// Run the given closure `f` with an instance of this runtime.
	fn with_instance<R, F>(
		&self,
		ext: &mut dyn Externalities,
		heap_alloc_strategy: HeapAllocStrategy,
		f: F,
	) -> Result<R, Error>
	where
		F: FnOnce(
			&dyn WasmModule,
			&mut dyn WasmInstance,
			Option<&RuntimeVersion>,
			&mut dyn Externalities,
		) -> Result<R, Error>,
	{
		// Find a free instance
		let instance = self
			.instances
			.iter()
			.enumerate()
			.find_map(|(index, i)| i.try_lock().map(|i| (index, i)));

		match instance {
			Some((index, mut locked)) => {
				let (mut instance, new_inst) =
					locked.take().map(|r| Ok((r, false))).unwrap_or_else(|| {
						self.module.new_instance(heap_alloc_strategy).map(|i| (i, true))
					})?;

				// Update the heap allocation strategy for pooled instances, since the
				// caller may need different memory limits than what the instance was
				// originally created with.
				if !new_inst {
					instance.set_heap_alloc_strategy(heap_alloc_strategy);
				}

				let result = f(&*self.module, &mut *instance, self.version.as_ref(), ext);
				if let Err(e) = &result {
					if new_inst {
						tracing::warn!(
							target: "wasm-runtime",
							error = %e,
							"Fresh runtime instance failed",
						)
					} else {
						tracing::warn!(
							target: "wasm-runtime",
							error = %e,
							"Evicting failed runtime instance",
						);
					}
				} else {
					*locked = Some(instance);

					if new_inst {
						tracing::debug!(
							target: "wasm-runtime",
							"Allocated WASM instance {}/{}",
							index + 1,
							self.instances.len(),
						);
					}
				}

				result
			},
			None => {
				tracing::warn!(target: "wasm-runtime", "Ran out of free WASM instances");

				// Allocate a new instance
				let mut instance = self.module.new_instance(heap_alloc_strategy)?;

				f(&*self.module, &mut *instance, self.version.as_ref(), ext)
			},
		}
	}

	/// Run the given closure `f` with a fresh timed instance of this runtime.
	///
	/// Blocks until the timed module is available — it may still be compiling in the
	/// background. Timed instances are not pooled: every call gets a fresh instance.
	fn with_timed_instance<R, F>(
		&self,
		ext: &mut dyn Externalities,
		heap_alloc_strategy: HeapAllocStrategy,
		f: F,
	) -> Result<R, Error>
	where
		F: FnOnce(
			&mut dyn TimedWasmInstance,
			Option<&RuntimeVersion>,
			&mut dyn Externalities,
		) -> Result<R, Error>,
	{
		let module = self.timed_module.wait().as_ref().map_err(|e| Error::from(e.clone()))?;
		let mut instance = module.new_instance(heap_alloc_strategy)?;

		f(&mut *instance, self.version.as_ref(), ext)
	}
}

/// Cache for the runtimes.
///
/// When an instance is requested for the first time it is added to this cache. Metadata is kept
/// with the instance so that it can be efficiently reinitialized.
///
/// When using the Wasmi interpreter execution method, the metadata includes the initial memory and
/// values of mutable globals. Follow-up requests to fetch a runtime return this one instance with
/// the memory reset to the initial memory. So, one runtime instance is reused for every fetch
/// request.
///
/// The size of cache is configurable via the cli option `--runtime-cache-size`.
pub struct RuntimeCache {
	/// A cache of runtimes along with metadata.
	///
	/// Runtimes sorted by recent usage. The most recently used is at the front.
	runtimes: Mutex<LruMap<VersionedRuntimeId, Arc<VersionedRuntime>>>,
	/// The size of the instances cache for each runtime.
	max_runtime_instances: usize,
	cache_path: Option<PathBuf>,
}

impl RuntimeCache {
	/// Creates a new instance of a runtimes cache.
	///
	/// `max_runtime_instances` specifies the number of instances per runtime preserved in an
	/// in-memory cache.
	///
	/// `cache_path` allows to specify an optional directory where the executor can store files
	/// for caching.
	///
	/// `runtime_cache_size` specifies the number of different runtimes versions preserved in an
	/// in-memory cache, must always be at least 1.
	pub fn new(
		max_runtime_instances: usize,
		cache_path: Option<PathBuf>,
		runtime_cache_size: u8,
	) -> RuntimeCache {
		let cap = ByLength::new(runtime_cache_size.max(1) as u32);
		RuntimeCache { runtimes: Mutex::new(LruMap::new(cap)), max_runtime_instances, cache_path }
	}

	/// Prepares a WASM module instance and executes given function for it.
	///
	/// This uses internal cache to find available instance or create a new one.
	/// # Parameters
	///
	/// `runtime_code` - The runtime wasm code used setup the runtime.
	///
	/// `ext` - The externalities to access the state.
	///
	/// `wasm_method` - Type of WASM backend to use.
	///
	/// `heap_alloc_strategy` - The heap allocation strategy to use.
	///
	/// `allow_missing_func_imports` - Ignore missing function imports.
	///
	/// `f` - Function to execute.
	///
	/// `H` - A compile-time list of host functions to expose to the runtime.
	///
	/// # Returns result of `f` wrapped in an additional result.
	/// In case of failure one of two errors can be returned:
	///
	/// `Err::RuntimeConstruction` is returned for runtime construction issues.
	///
	/// `Error::InvalidMemoryReference` is returned if no memory export with the
	/// identifier `memory` can be found in the runtime.
	pub fn with_instance<'c, H, R, F>(
		&self,
		runtime_code: &'c RuntimeCode<'c>,
		ext: &mut dyn Externalities,
		wasm_method: WasmExecutionMethod,
		heap_alloc_strategy: HeapAllocStrategy,
		allow_missing_func_imports: bool,
		f: F,
	) -> Result<Result<R, Error>, Error>
	where
		H: HostFunctions,
		F: FnOnce(
			&dyn WasmModule,
			&mut dyn WasmInstance,
			Option<&RuntimeVersion>,
			&mut dyn Externalities,
		) -> Result<R, Error>,
	{
		let versioned_runtime = self.versioned_runtime::<H>(
			runtime_code,
			ext,
			wasm_method,
			heap_alloc_strategy,
			allow_missing_func_imports,
		)?;

		Ok(versioned_runtime.with_instance(ext, heap_alloc_strategy, f))
	}

	/// Prepares a fresh timed WASM module instance and executes given function for it.
	///
	/// Same as [`Self::with_instance`], but `f` gets a timed instance whose calls are
	/// interrupted on timeout. May block until the timed module is available — it may still be
	/// compiling in the background.
	///
	/// NOTE: engines without an execution-interruption mechanism (PolkaVM) ignore the timeout
	/// and run uncapped.
	pub fn with_timed_instance<'c, H, R, F>(
		&self,
		runtime_code: &'c RuntimeCode<'c>,
		ext: &mut dyn Externalities,
		wasm_method: WasmExecutionMethod,
		heap_alloc_strategy: HeapAllocStrategy,
		allow_missing_func_imports: bool,
		f: F,
	) -> Result<Result<R, Error>, Error>
	where
		H: HostFunctions,
		F: FnOnce(
			&mut dyn TimedWasmInstance,
			Option<&RuntimeVersion>,
			&mut dyn Externalities,
		) -> Result<R, Error>,
	{
		let versioned_runtime = self.versioned_runtime::<H>(
			runtime_code,
			ext,
			wasm_method,
			heap_alloc_strategy,
			allow_missing_func_imports,
		)?;

		Ok(versioned_runtime.with_timed_instance(ext, heap_alloc_strategy, f))
	}

	/// Get the cached [`VersionedRuntime`] for the given code, creating and caching it first if
	/// needed.
	fn versioned_runtime<H>(
		&self,
		runtime_code: &RuntimeCode,
		ext: &mut dyn Externalities,
		wasm_method: WasmExecutionMethod,
		heap_alloc_strategy: HeapAllocStrategy,
		allow_missing_func_imports: bool,
	) -> Result<Arc<VersionedRuntime>, WasmError>
	where
		H: HostFunctions,
	{
		let code_hash = &runtime_code.hash;

		let versioned_runtime_id = VersionedRuntimeId { code_hash: code_hash.clone(), wasm_method };

		// The lock is released when the guard goes out of scope, prior to calling any
		// instance method.
		let mut runtimes = self.runtimes.lock();
		if let Some(versioned_runtime) = runtimes.get(&versioned_runtime_id) {
			return Ok(versioned_runtime.clone());
		}

		let code = runtime_code.fetch_runtime_code().ok_or(WasmError::CodeNotFound)?;

		let time = std::time::Instant::now();

		let result = create_versioned_wasm_runtime::<H>(
			&code,
			ext,
			wasm_method,
			heap_alloc_strategy,
			allow_missing_func_imports,
			self.max_runtime_instances,
			self.cache_path.as_deref(),
		);

		match result {
			Ok(ref result) => {
				tracing::debug!(
					target: "wasm-runtime",
					"Prepared new runtime version {:?} in {} ms.",
					result.version,
					time.elapsed().as_millis(),
				);
			},
			Err(ref err) => {
				tracing::warn!(target: "wasm-runtime", error = ?err, "Cannot create a runtime");
			},
		}

		let versioned_runtime = Arc::new(result?);

		// Save new versioned wasm runtime in cache
		runtimes.insert(versioned_runtime_id, versioned_runtime.clone());

		Ok(versioned_runtime)
	}
}

fn wasmtime_config(
	heap_alloc_strategy: HeapAllocStrategy,
	instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy,
	allow_missing_func_imports: bool,
	cache_path: Option<&Path>,
) -> sc_executor_wasmtime::Config {
	sc_executor_wasmtime::Config {
		allow_missing_func_imports,
		cache_path: cache_path.map(ToOwned::to_owned),
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy,
			instantiation_strategy,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: true,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	}
}

/// Create a wasm runtime with the given `code`.
pub fn create_wasm_runtime_with_code<H>(
	wasm_method: WasmExecutionMethod,
	heap_alloc_strategy: HeapAllocStrategy,
	blob: RuntimeBlob,
	allow_missing_func_imports: bool,
	cache_path: Option<&Path>,
) -> Result<Box<dyn WasmModule>, WasmError>
where
	H: HostFunctions,
{
	if let Some(blob) = blob.as_polkavm_blob() {
		return sc_executor_polkavm::create_runtime::<H>(blob)
			.map(|pre| -> Box<dyn WasmModule> { Box::new(pre) });
	}

	match wasm_method {
		WasmExecutionMethod::Compiled { instantiation_strategy } => {
			sc_executor_wasmtime::create_runtime::<H>(
				blob,
				wasmtime_config(
					heap_alloc_strategy,
					instantiation_strategy,
					allow_missing_func_imports,
					cache_path,
				),
			)
			.map(|runtime| -> Box<dyn WasmModule> { Box::new(runtime) })
		},
	}
}

/// Like [`create_wasm_runtime_with_code`], but also provides the runtime's timed counterpart.
fn create_wasm_runtime_with_timed<H>(
	wasm_method: WasmExecutionMethod,
	heap_alloc_strategy: HeapAllocStrategy,
	blob: RuntimeBlob,
	allow_missing_func_imports: bool,
	cache_path: Option<&Path>,
) -> Result<(Box<dyn WasmModule>, TimedModuleSlot), WasmError>
where
	H: HostFunctions,
{
	if let Some(program_blob) = blob.as_polkavm_blob() {
		static POLKAVM_TIMED_WARN: std::sync::Once = std::sync::Once::new();
		POLKAVM_TIMED_WARN.call_once(|| {
			tracing::warn!(
				target: "wasm-runtime",
				"PolkaVM does not support execution timeouts; timed runtime calls will run \
				 uncapped",
			);
		});

		// One compile, two handles: `InstancePre` clones via `Arc`.
		let pre = sc_executor_polkavm::create_runtime::<H>(program_blob)?;
		return Ok((
			Box::new(pre.clone()),
			Arc::new(OnceLock::from(Ok(Box::new(pre) as Box<dyn TimedWasmModule>))),
		));
	}

	let timed_blob = blob.clone();
	let runtime = create_wasm_runtime_with_code::<H>(
		wasm_method,
		heap_alloc_strategy,
		blob,
		allow_missing_func_imports,
		cache_path,
	)?;

	let timed_module: TimedModuleSlot = Arc::new(OnceLock::new());
	let WasmExecutionMethod::Compiled { instantiation_strategy } = wasm_method;

	spawn_timed_compilation::<H>(
		timed_blob,
		// Reusing `cache_path` is safe: wasmtime keys on-disk artifacts on the full config,
		// including the epoch-interruption flag forced by `create_timed_runtime`.
		wasmtime_config(
			heap_alloc_strategy,
			instantiation_strategy,
			allow_missing_func_imports,
			cache_path,
		),
		timed_module.clone(),
	);

	Ok((runtime, timed_module))
}

/// Compile the timed counterpart of a wasmtime runtime in a detached background thread,
/// eventually filling `slot`.
///
/// `slot` is always set — even on spawn failure or compile panic — so [`OnceLock::wait`] on it
/// cannot hang.
fn spawn_timed_compilation<H>(
	blob: RuntimeBlob,
	config: sc_executor_wasmtime::Config,
	slot: TimedModuleSlot,
) where
	H: HostFunctions,
{
	let thread = std::thread::Builder::new().name("wasm-timed-compile".into()).spawn({
		let slot = slot.clone();
		move || {
			// Substrate's panic hook aborts the process; force unwinding to contain a compile
			// panic to this thread.
			let _guard = sp_panic_handler::AbortGuard::force_unwind();
			let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
				sc_executor_wasmtime::create_timed_runtime::<H>(blob, config)
			}))
			.unwrap_or_else(|e| {
				let message = if let Some(msg) = e.downcast_ref::<String>() {
					msg.clone()
				} else if let Some(msg) = e.downcast_ref::<&'static str>() {
					msg.to_string()
				} else {
					"unknown panic".into()
				};
				Err(WasmError::Other(format!("timed runtime compilation panicked: {message}")))
			})
			.map(|runtime| -> Box<dyn TimedWasmModule> { Box::new(runtime) });

			match result {
				Ok(_) => {
					tracing::debug!(target: "wasm-runtime", "Prepared new timed runtime.");
				},
				Err(ref err) => {
					tracing::warn!(
						target: "wasm-runtime",
						error = ?err,
						"Cannot create a timed runtime",
					);
				},
			}

			let _ = slot.set(result);
		}
	});

	if let Err(err) = thread {
		let _ = slot.set(Err(WasmError::Other(format!(
			"failed to spawn the timed runtime compilation thread: {err}"
		))));
	}
}

fn decode_version(mut version: &[u8]) -> Result<RuntimeVersion, WasmError> {
	Decode::decode(&mut version).map_err(|_| {
		WasmError::Instantiation(
			"failed to decode \"Core_version\" result using old runtime version".into(),
		)
	})
}

fn decode_runtime_apis(apis: &[u8]) -> Result<Vec<([u8; 8], u32)>, WasmError> {
	use sp_api::RUNTIME_API_INFO_SIZE;

	apis.chunks(RUNTIME_API_INFO_SIZE)
		.map(|chunk| {
			// `chunk` can be less than `RUNTIME_API_INFO_SIZE` if the total length of `apis`
			// doesn't completely divide by `RUNTIME_API_INFO_SIZE`.
			<[u8; RUNTIME_API_INFO_SIZE]>::try_from(chunk)
				.map(sp_api::deserialize_runtime_api_info)
				.map_err(|_| WasmError::Other("a clipped runtime api info declaration".to_owned()))
		})
		.collect::<Result<Vec<_>, WasmError>>()
}

/// Take the runtime blob and scan it for the custom wasm sections containing the version
/// information and construct the `RuntimeVersion` from them.
///
/// If there are no such sections, it returns `None`. If there is an error during decoding those
/// sections, `Err` will be returned.
pub fn read_embedded_version(blob: &RuntimeBlob) -> Result<Option<RuntimeVersion>, WasmError> {
	if let Some(mut version_section) = blob.custom_section_contents("runtime_version") {
		let apis = blob
			.custom_section_contents("runtime_apis")
			.map(decode_runtime_apis)
			.transpose()?
			.map(Into::into);

		let core_version = apis.as_ref().and_then(sp_version::core_version_from_apis);
		// We do not use `RuntimeVersion::decode` here because that `decode_version` relies on
		// presence of a special API in the `apis` field to treat the input as a non-legacy version.
		// However the structure found in the `runtime_version` always contain an empty `apis`
		// field. Therefore the version read will be mistakenly treated as an legacy one.
		let mut decoded_version = sp_version::RuntimeVersion::decode_with_version_hint(
			&mut version_section,
			core_version,
		)
		.map_err(|_| WasmError::Instantiation("failed to decode version section".into()))?;

		if let Some(apis) = apis {
			decoded_version.apis = apis;
		}

		Ok(Some(decoded_version))
	} else {
		Ok(None)
	}
}

fn create_versioned_wasm_runtime<H>(
	code: &[u8],
	ext: &mut dyn Externalities,
	wasm_method: WasmExecutionMethod,
	heap_alloc_strategy: HeapAllocStrategy,
	allow_missing_func_imports: bool,
	max_instances: usize,
	cache_path: Option<&Path>,
) -> Result<VersionedRuntime, WasmError>
where
	H: HostFunctions,
{
	// The incoming code may be actually compressed. We decompress it here and then work with
	// the uncompressed code from now on.
	let blob = sc_executor_common::runtime_blob::RuntimeBlob::uncompress_if_needed(code)?;

	// Use the runtime blob to scan if there is any metadata embedded into the wasm binary
	// pertaining to runtime version. We do it before consuming the runtime blob for creating the
	// runtime.
	let mut version = read_embedded_version(&blob)?;

	let (runtime, timed_module) = create_wasm_runtime_with_timed::<H>(
		wasm_method,
		heap_alloc_strategy,
		blob,
		allow_missing_func_imports,
		cache_path,
	)?;

	// If the runtime blob doesn't embed the runtime version then use the legacy version query
	// mechanism: call the runtime.
	if version.is_none() {
		// Call to determine runtime version.
		let version_result = {
			// `ext` is already implicitly handled as unwind safe, as we store it in a global
			// variable.
			let mut ext = AssertUnwindSafe(ext);

			// The following unwind safety assertion is OK because if the method call panics, the
			// runtime will be dropped.
			let runtime = AssertUnwindSafe(runtime.as_ref());
			crate::executor::with_externalities_safe(&mut **ext, move || {
				runtime.new_instance(heap_alloc_strategy)?.call("Core_version".into(), &[])
			})
			.map_err(|_| WasmError::Instantiation("panic in call to get runtime version".into()))?
		};

		if let Ok(version_buf) = version_result {
			version = Some(decode_version(&version_buf)?)
		}
	}

	let mut instances = Vec::with_capacity(max_instances);
	instances.resize_with(max_instances, || Mutex::new(None));

	Ok(VersionedRuntime { module: runtime, timed_module, version, instances })
}

#[cfg(test)]
mod tests {
	extern crate alloc;

	use super::*;
	use alloc::borrow::Cow;
	use codec::Encode;
	use sp_api::{Core, RuntimeApiInfo};
	use sp_version::{create_apis_vec, RuntimeVersion};
	use sp_wasm_interface::HostFunctions;
	use substrate_test_runtime::Block;

	#[derive(Encode)]
	pub struct OldRuntimeVersion {
		pub spec_name: Cow<'static, str>,
		pub impl_name: Cow<'static, str>,
		pub authoring_version: u32,
		pub spec_version: u32,
		pub impl_version: u32,
		pub apis: sp_version::ApisVec,
	}

	#[test]
	fn host_functions_are_equal() {
		let host_functions = sp_io::SubstrateHostFunctions::host_functions();

		let equal = &host_functions[..] == &host_functions[..];
		assert!(equal, "Host functions are not equal");
	}

	#[test]
	fn old_runtime_version_decodes() {
		let old_runtime_version = OldRuntimeVersion {
			spec_name: "test".into(),
			impl_name: "test".into(),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: create_apis_vec!([(<dyn Core::<Block>>::ID, 1)]),
		};

		let version = decode_version(&old_runtime_version.encode()).unwrap();
		assert_eq!(1, version.transaction_version);
		assert_eq!(0, version.system_version);
	}

	#[test]
	fn old_runtime_version_decodes_fails_with_version_3() {
		let old_runtime_version = OldRuntimeVersion {
			spec_name: "test".into(),
			impl_name: "test".into(),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: create_apis_vec!([(<dyn Core::<Block>>::ID, 3)]),
		};

		decode_version(&old_runtime_version.encode()).unwrap_err();
	}

	#[test]
	fn new_runtime_version_decodes() {
		let old_runtime_version = RuntimeVersion {
			spec_name: "test".into(),
			impl_name: "test".into(),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: create_apis_vec!([(<dyn Core::<Block>>::ID, 3)]),
			transaction_version: 3,
			system_version: 4,
		};

		let version = decode_version(&old_runtime_version.encode()).unwrap();
		assert_eq!(3, version.transaction_version);
		assert_eq!(0, version.system_version);

		let old_runtime_version = RuntimeVersion {
			spec_name: "test".into(),
			impl_name: "test".into(),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: create_apis_vec!([(<dyn Core::<Block>>::ID, 4)]),
			transaction_version: 3,
			system_version: 4,
		};

		let version = decode_version(&old_runtime_version.encode()).unwrap();
		assert_eq!(3, version.transaction_version);
		assert_eq!(4, version.system_version);
	}

	#[test]
	fn embed_runtime_version_works() {
		let wasm = sp_maybe_compressed_blob::decompress(
			substrate_test_runtime::wasm_binary_unwrap(),
			sp_maybe_compressed_blob::CODE_BLOB_BOMB_LIMIT,
		)
		.expect("Decompressing works");
		let runtime_version = RuntimeVersion {
			spec_name: "test_replace".into(),
			impl_name: "test_replace".into(),
			authoring_version: 100,
			spec_version: 100,
			impl_version: 100,
			apis: create_apis_vec!([(<dyn Core::<Block>>::ID, 4)]),
			transaction_version: 100,
			system_version: 1,
		};

		let embedded = sp_version::embed::embed_runtime_version(&wasm, runtime_version.clone())
			.expect("Embedding works");

		let blob = RuntimeBlob::new(&embedded).expect("Embedded blob is valid");
		let read_version = read_embedded_version(&blob)
			.ok()
			.flatten()
			.expect("Reading embedded version works");

		assert_eq!(runtime_version, read_version);
	}
}
