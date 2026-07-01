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

//! Host-side PolkaVM backend for the [`sp_virtualization`] host functions.
//!
//! Provides the concrete [`VirtManager`] that drives `polkavm` to compile, instantiate
//! and execute programs on behalf of the runtime. Register it with the externalities via
//! [`sp_virtualization::VirtManagerExt::new`] — or use the [`default_extension`] helper
//! and [`ExtensionsFactory`] convenience types defined below.

use polkavm::{
	CacheModel, CompileError, Config, CostModelKind, Engine, GasMeteringKind, InterruptKind,
	MemoryAccessError, Module, ModuleConfig, ProgramCounter, RawInstance, Reg,
};
use polkavm_common::{
	program::{asm, InstructionSetKind},
	writer::ProgramBlobBuilder,
};
use sp_externalities::Extensions;
use sp_runtime::traits::{Block as BlockT, NumberFor};
use sp_virtualization::{
	DestroyError, ExecBuffer, ExecError, ExecStatus, InstanceId, InstantiateError, MemoryError,
	ModuleError, ModuleId, SyscallSymbol, VirtManagerBackend, VirtManagerExt, LOG_TARGET,
};
use std::{
	collections::HashMap,
	sync::{Arc, LazyLock},
};

/// Build a fresh [`VirtManagerExt`] backed by a default [`VirtManager`].
///
/// Use this where you would otherwise hand-roll
/// `VirtManagerExt::new(VirtManager::default())` — e.g. when registering the
/// extension directly on a `TestExternalities` or an `Extensions` set.
///
/// Constructing the [`VirtManager`] builds and warms the engine (see
/// [`VirtManager::default`]), so the returned extension is always backed by a warm
/// sandbox pool.
pub fn default_extension() -> VirtManagerExt {
	VirtManagerExt::new(VirtManager::default())
}

/// An [`sc_client_api::execution_extensions::ExtensionsFactory`] that registers a fresh
/// [`VirtManagerExt`] (backed by a default [`VirtManager`]) for every runtime call.
///
/// Plug this into a client via
/// `client.execution_extensions().set_extensions_factory(ExtensionsFactory)`.
/// [`VirtManagerExt`] cannot implement `Default` (its backend lives in this crate, and
/// `sp-virtualization` is intentionally backend-free), so the stock
/// [`sc_client_api::execution_extensions::ExtensionBeforeBlock`] helper cannot be used.
#[derive(Default)]
pub struct ExtensionsFactory;

impl<Block: BlockT> sc_client_api::execution_extensions::ExtensionsFactory<Block>
	for ExtensionsFactory
{
	fn extensions_for(&self, _: Block::Hash, _: NumberFor<Block>) -> Extensions {
		let mut exts = Extensions::new();
		exts.register(default_extension());
		exts
	}
}

/// Maximum number of virtualization instances alive at once.
///
/// Each live instance pins a sandbox worker process and its memory, so the number alive at once
/// must be bounded. One instance is live per nested contract frame, so the bound is the maximum
/// contract call-stack depth and must stay `>=` the consumer's maximum call depth (for
/// pallet-revive, `CALL_STACK_DEPTH + 1`, currently 26).
///
/// It serves two roles:
/// - [`warm_up_sandbox_pool`] pre-spawns this many sandbox workers so a fully-nested call always
///   finds one warm and never has to spawn mid-validation. `set_worker_count` is *per core*, so
///   [`ENGINE`]'s constructor divides this across cores (rounding up).
/// - [`VirtManager::instantiate`] enforces it as a hard cap, returning
///   `InstantiateError::TooManyInstances` once that many instances are live.
const MAX_LIVE_INSTANCES: usize = 30;

/// The single, process-global PolkaVM engine used for everything.
///
/// A common engine lets PolkaVM keep a warm pool of sandbox workers and amortise startup
/// costs across instances, even when those instances run different code. The engine is built
/// *and* its sandbox pool warmed in one step on first access, so it is never observable
/// un-warmed; `LazyLock` guarantees that happens exactly once per process. Warm-up panics on
/// failure (see [`warm_up_sandbox_pool`]) — a node that cannot build its pool should die at
/// startup rather than time out every validation.
static ENGINE: LazyLock<Engine> = LazyLock::new(|| {
	let mut config = Config::from_env().expect("Invalid config.");
	// `set_worker_count` caps the sandbox cache per core, so divide the total target across
	// cores (rounding up) — summed over all cores the caches then hold >= MAX_LIVE_INSTANCES.
	let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
	config.set_worker_count(MAX_LIVE_INSTANCES.div_ceil(cores));
	config.set_default_cost_model(Some(CostModelKind::Full(CacheModel::L2Hit)));
	let engine = Engine::new(&config).expect("Failed to initialize PolkaVM.");
	warm_up_sandbox_pool(&engine);
	engine
});

/// Build and warm the engine now, rather than on first use.
///
/// A no-op after the first call — `LazyLock` builds and warms at most once.
/// [`VirtManager::default`] calls this on every construction, so a warm pool is guaranteed however
/// a manager is built and no consumer has to remember to opt in. Call it explicitly at process
/// startup — in particular at PVF execute-worker startup — to move the one-time build+warm off the
/// first validation's timing, where the extension is otherwise first constructed inside the timed
/// section. The collator needs no such explicit call: it builds its first extension through an
/// untimed startup/sync runtime call, so the warm-up already lands off the authoring deadline.
pub fn init() {
	LazyLock::force(&ENGINE);
}

/// The process-global engine.
///
/// Forcing the `LazyLock` builds the engine and warms its sandbox pool on first access, so a
/// ready, warm engine is returned regardless of whether anything initialised it beforehand.
fn engine() -> &'static Engine {
	LazyLock::force(&ENGINE)
}

/// Pre-spawn [`MAX_LIVE_INSTANCES`] sandbox workers so a max-depth nest finds them warm.
///
/// `instantiate` acquires a sandbox (spawning a fresh worker process when none is cached)
/// and holds it for the instance's lifetime; dropping the instance recycles the sandbox into
/// the engine's per-core cache. We hold [`MAX_LIVE_INSTANCES`] instances at once so each
/// forces a *distinct* sandbox to spawn (a sequential loop would just reuse one); they then
/// recycle across the per-core caches the same way a deeply-nested call's frames distribute,
/// so a nest up to that depth finds a warm sandbox wherever PolkaVM places each frame. The
/// instances are never run; a one-instruction module is enough to make `instantiate` spawn a
/// sandbox. Panics on any failure — a node that cannot build its sandbox pool cannot run JIT
/// contracts, so it should fail at startup rather than limp on and time out every validation.
fn warm_up_sandbox_pool(engine: &Engine) {
	// A single `ret` with no exports is enough: the module is only ever instantiated (which
	// spawns and loads a sandbox), never run.
	let mut builder = ProgramBlobBuilder::new(InstructionSetKind::ReviveV1);
	builder.set_code(&[asm::ret()], &[]);
	let blob = builder.into_vec().expect("warm-up blob is a constant valid program; qed");

	let mut module_config = ModuleConfig::new();
	module_config.set_gas_metering(Some(GasMeteringKind::Sync));
	let module = Module::new(engine, &module_config, blob.into())
		.expect("warm-up program is a constant the engine must be able to compile; qed");

	// Held all at once (not a sequential loop) so each forces a *distinct* sandbox to spawn;
	// they recycle into the per-core caches when this `Vec` drops at the end of the function.
	let _warm: Vec<_> = (0..MAX_LIVE_INSTANCES)
		.map(|_| module.instantiate().expect("failed to spawn a PolkaVM sandbox worker"))
		.collect();
	log::debug!(target: LOG_TARGET, "Sandbox warm-up: pre-spawned {MAX_LIVE_INSTANCES} sandbox workers");
}

fn map_memory_error(error: MemoryAccessError) -> MemoryError {
	match error {
		MemoryAccessError::OutOfRangeAccess { .. } | MemoryAccessError::MemoryLimitReached => {
			MemoryError::OutOfBounds
		},
		MemoryAccessError::Error(error) => {
			panic!("Error accessing PolkaVM memory: {error}. This is a bug.");
		},
	}
}

/// A compiled module together with lookup tables derived from it once at compile time.
///
/// Precomputing these avoids an O(n) `exports()` scan on every `prepare` call and a
/// re-copy of the import symbol bytes on every syscall.
struct CompiledModule {
	module: Module,
	/// Export symbol → program counter. Consulted by `prepare`.
	exports: HashMap<Vec<u8>, ProgramCounter>,
	/// Preassembled `SyscallSymbol` for each import, indexed by hostcall index.
	imports: Vec<SyscallSymbol>,
}

impl CompiledModule {
	fn new(module: Module) -> Result<Self, ModuleError> {
		let exports = module
			.exports()
			.map(|e| (e.symbol().as_bytes().to_vec(), e.program_counter()))
			.collect();

		// `ImportsIter` yields `Option<ProgramSymbol>` (None on malformed offsets);
		// we also reject any symbol longer than our fixed-size `SyscallSymbol` buffer
		// so the Ecalli hot path can just index into the vec.
		let imports: Vec<SyscallSymbol> = module
			.imports()
			.into_iter()
			.map(|symbol| {
				let symbol = symbol.ok_or(ModuleError::InvalidImage)?;
				SyscallSymbol::new(symbol.as_bytes()).ok_or(ModuleError::InvalidImage)
			})
			.collect::<Result<_, _>>()?;

		Ok(Self { module, exports, imports })
	}
}

/// The state an instance can be in.
enum InstanceState {
	/// Idle — awaiting a `prepare` call before it can be run.
	Idle(RawInstance),
	/// Ready — prepared for execution, or suspended mid-execution at a syscall.
	Ready(RawInstance),
}

/// An instance together with the compiled module it was instantiated from.
///
/// The module handle is kept here so the hot paths can consult the precomputed
/// export/import tables without going through `RawInstance::module()`.
struct ManagedInstance {
	state: InstanceState,
	module: Arc<CompiledModule>,
}

/// Manages virtualization instances and their lifecycle.
///
/// Instance and module IDs are assigned deterministically from incrementing counters,
/// ensuring no non-determinism across different executions.
///
/// NOTE: The per-instance `cache` deduplicates modules within the lifetime of one
/// `VirtManager` (i.e. one externalities extension, i.e. one block). Cross-block
/// reuse is deferred to PolkaVM's built-in on-disk persistent cache.
pub struct VirtManager {
	instances: HashMap<InstanceId, ManagedInstance>,
	modules: HashMap<ModuleId, Arc<CompiledModule>>,
	cache: HashMap<Vec<u8>, Arc<CompiledModule>>,
	instance_counter: u32,
	module_counter: u32,
}

impl Default for VirtManager {
	fn default() -> Self {
		// Warm the sandbox pool on construction so no manager — however it was built, including
		// through this public constructor — can run contracts against a cold pool. Idempotent:
		// the actual warm-up happens once per process, later constructions are a cheap no-op.
		init();
		Self {
			instances: HashMap::new(),
			modules: HashMap::new(),
			cache: HashMap::new(),
			instance_counter: 0,
			module_counter: 0,
		}
	}
}

impl VirtManager {
	fn next_module_id(&mut self) -> ModuleId {
		let old = self.module_counter;
		self.module_counter = old + 1;
		ModuleId::from(old)
	}

	fn next_instance_id(&mut self) -> InstanceId {
		let old = self.instance_counter;
		self.instance_counter = old + 1;
		InstanceId::from(old)
	}

	fn prepare_impl(
		managed: ManagedInstance,
		function: &[u8],
	) -> (ManagedInstance, Result<(), ExecError>) {
		let ManagedInstance { state, module } = managed;
		let mut instance = match state {
			InstanceState::Idle(i) => i,
			ready @ InstanceState::Ready(_) => {
				return (ManagedInstance { state: ready, module }, Err(ExecError::InvalidInstance));
			},
		};
		match module.exports.get(function).copied() {
			Some(pc) => {
				instance.prepare_call_untyped(pc, &[]);
				(ManagedInstance { state: InstanceState::Ready(instance), module }, Ok(()))
			},
			None => {
				log::debug!(
					target: LOG_TARGET,
					"Export not found: {}",
					String::from_utf8_lossy(function),
				);
				(
					ManagedInstance { state: InstanceState::Idle(instance), module },
					Err(ExecError::InvalidImage),
				)
			},
		}
	}

	fn run_impl(
		managed: ManagedInstance,
		gas_left: i64,
		a0: u64,
	) -> (ManagedInstance, Result<(ExecStatus, ExecBuffer), ExecError>) {
		let ManagedInstance { state, module } = managed;
		let mut instance = match state {
			InstanceState::Ready(i) => i,
			idle @ InstanceState::Idle(_) => {
				return (ManagedInstance { state: idle, module }, Err(ExecError::InvalidInstance));
			},
		};

		instance.set_reg(Reg::A0, a0);
		instance.set_gas(gas_left);

		let interrupt = match instance.run() {
			Ok(interrupt) => interrupt,
			Err(err) => panic!("Polkavm failed during execution: {err}. This is a bug."),
		};

		match interrupt {
			InterruptKind::Finished => {
				let gas_left = instance.gas();
				(
					ManagedInstance { state: InstanceState::Idle(instance), module },
					Ok((ExecStatus::Finished, ExecBuffer { gas_left, ..Default::default() })),
				)
			},
			InterruptKind::Trap => (
				ManagedInstance { state: InstanceState::Idle(instance), module },
				Err(ExecError::Trap),
			),
			InterruptKind::NotEnoughGas => (
				ManagedInstance { state: InstanceState::Idle(instance), module },
				Err(ExecError::OutOfGas),
			),
			InterruptKind::Step | InterruptKind::Segfault(_) => {
				unreachable!("PolkaVM is configured per config not to emit Step or Segfault; qed");
			},
			InterruptKind::Ecalli(hostcall_index) => {
				let Some(syscall_symbol) = module.imports.get(hostcall_index as usize).copied()
				else {
					return (
						ManagedInstance { state: InstanceState::Idle(instance), module },
						Err(ExecError::InvalidImage),
					);
				};
				let gas_left = instance.gas();
				let a0 = instance.reg(Reg::A0);
				let a1 = instance.reg(Reg::A1);
				let a2 = instance.reg(Reg::A2);
				let a3 = instance.reg(Reg::A3);
				let a4 = instance.reg(Reg::A4);
				let a5 = instance.reg(Reg::A5);
				(
					ManagedInstance { state: InstanceState::Ready(instance), module },
					Ok((
						ExecStatus::Syscall,
						ExecBuffer { gas_left, syscall_symbol, a0, a1, a2, a3, a4, a5 },
					)),
				)
			},
		}
	}
}

impl VirtManagerBackend for VirtManager {
	fn compile(
		&mut self,
		program: &[u8],
		identifier: Option<&[u8]>,
	) -> Result<ModuleId, ModuleError> {
		let mut module_config = ModuleConfig::new();
		module_config.set_gas_metering(Some(GasMeteringKind::Sync));
		let module =
			Module::new(engine(), &module_config, program.into()).map_err(|err| match err {
				CompileError::ValidationFailed(err) => {
					log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
					ModuleError::InvalidImage
				},
				CompileError::Error(err) => {
					panic!("Polkavm failed during compilation: {err}. This is a bug.");
				},
			})?;
		let compiled = Arc::new(CompiledModule::new(module)?);

		let module_id = self.next_module_id();

		if let Some(identifier) = identifier {
			self.cache.insert(identifier.to_vec(), compiled.clone());
		}
		self.modules.insert(module_id, compiled);

		Ok(module_id)
	}

	fn lookup(&mut self, identifier: &[u8]) -> Result<ModuleId, ModuleError> {
		let compiled = self.cache.get(identifier).cloned().ok_or(ModuleError::NotCached)?;
		let module_id = self.next_module_id();
		self.modules.insert(module_id, compiled);
		Ok(module_id)
	}

	fn instantiate(&mut self, module_id: ModuleId) -> Result<InstanceId, InstantiateError> {
		if self.instances.len() >= MAX_LIVE_INSTANCES {
			log::error!(target: LOG_TARGET, "live-instance limit ({MAX_LIVE_INSTANCES}) reached");
			return Err(InstantiateError::TooManyInstances);
		}

		let compiled = self.modules.get(&module_id).ok_or(InstantiateError::InvalidModule)?.clone();

		let instance = compiled.module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?;

		let instance_id = self.next_instance_id();

		self.instances.insert(
			instance_id,
			ManagedInstance { state: InstanceState::Idle(instance), module: compiled },
		);

		Ok(instance_id)
	}

	fn prepare(&mut self, instance_id: InstanceId, function: &[u8]) -> Result<(), ExecError> {
		let managed = self.instances.remove(&instance_id).ok_or(ExecError::InvalidInstance)?;
		let (managed, result) = Self::prepare_impl(managed, function);
		self.instances.insert(instance_id, managed);
		result
	}

	fn run(
		&mut self,
		instance_id: InstanceId,
		gas_left: i64,
		a0: u64,
	) -> Result<(ExecStatus, ExecBuffer), ExecError> {
		let managed = self.instances.remove(&instance_id).ok_or(ExecError::InvalidInstance)?;
		let (managed, result) = Self::run_impl(managed, gas_left, a0);
		self.instances.insert(instance_id, managed);
		result
	}

	fn destroy(&mut self, instance_id: InstanceId) -> Result<(), DestroyError> {
		if self.instances.remove(&instance_id).is_some() {
			Ok(())
		} else {
			Err(DestroyError::InvalidInstance)
		}
	}

	fn read_memory(
		&mut self,
		instance_id: InstanceId,
		offset: u32,
		dest: &mut [u8],
	) -> Result<(), MemoryError> {
		let Some(ManagedInstance { state: InstanceState::Ready(instance), .. }) =
			self.instances.get_mut(&instance_id)
		else {
			return Err(MemoryError::InvalidInstance);
		};
		instance.read_memory_into(offset, dest).map(|_| ()).map_err(map_memory_error)
	}

	fn write_memory(
		&mut self,
		instance_id: InstanceId,
		offset: u32,
		src: &[u8],
	) -> Result<(), MemoryError> {
		let Some(ManagedInstance { state: InstanceState::Ready(instance), .. }) =
			self.instances.get_mut(&instance_id)
		else {
			return Err(MemoryError::InvalidInstance);
		};
		instance.write_memory(offset, src).map_err(map_memory_error)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Two `VirtManager` instances must not share any cache state — confirms the cache lives on
	/// the struct, not in process-global storage.
	#[test]
	fn cache_does_not_leak_between_instances() {
		let program = sp_virtualization_test_fixture::binary();
		let key: &[u8] = b"some-key";

		let mut a = VirtManager::default();
		a.compile(program, Some(key)).unwrap();
		assert!(matches!(a.lookup(key), Ok(_)));

		let mut b = VirtManager::default();
		assert!(matches!(b.lookup(key), Err(ModuleError::NotCached)));
	}

	/// Passing `None` to `compile` must not populate the cache.
	#[test]
	fn compile_none_skips_cache() {
		let program = sp_virtualization_test_fixture::binary();
		let key: &[u8] = b"would-be-key";

		let mut m = VirtManager::default();
		m.compile(program, None).unwrap();
		assert!(matches!(m.lookup(key), Err(ModuleError::NotCached)));
	}

	/// The warm-up must build, compile and spawn its sandboxes without panicking — it fails the
	/// node otherwise. It spawns real sandboxes, so on Linux it relies on the `--privileged`
	/// workspace CI container (`clone3`/`unshare`); off Linux it falls back to the interpreter.
	#[test]
	fn warm_up_runs() {
		warm_up_sandbox_pool(engine());
	}

	/// `instantiate` refuses once [`MAX_LIVE_INSTANCES`] instances are live, and accepts again
	/// once one of them is destroyed.
	#[test]
	fn instantiate_enforces_live_instance_cap() {
		let program = sp_virtualization_test_fixture::binary();

		let mut m = VirtManager::default();
		let module_id = m.compile(program, None).unwrap();

		let ids: Vec<_> = (0..MAX_LIVE_INSTANCES)
			.map(|_| m.instantiate(module_id).expect("instantiation below the cap succeeds"))
			.collect();

		assert!(matches!(m.instantiate(module_id), Err(InstantiateError::TooManyInstances)));

		m.destroy(ids[0]).unwrap();
		assert!(m.instantiate(module_id).is_ok());
	}
}
