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

//! Manages virtualization instances. It is used by the host function **implementation**.

use crate::{
	host_functions::{ExecBuffer, ExecStatus},
	DestroyError, ExecError, InstanceId, InstantiateError, MemoryError, ModuleError, ModuleId,
	SyscallSymbol, LOG_TARGET, MAX_SYSCALL_SYMBOL_LEN,
};
use polkavm::{
	CacheModel, CompileError, Config, CostModelKind, Engine, GasMeteringKind, InterruptKind,
	MemoryAccessError, Module, ModuleConfig, ProgramCounter, RawInstance, Reg,
};
use std::{
	collections::HashMap,
	sync::{Arc, LazyLock, RwLock},
};

/// This is the single PolkaVM engine we use for everything.
///
/// By using a common engine we allow PolkaVM to use caching. This caching is important
/// to reduce startup costs. This is even the case when instances use different code.
static ENGINE: LazyLock<Engine> = LazyLock::new(|| {
	let mut config = Config::from_env().expect("Invalid config.");
	config.set_worker_count(10);
	config.set_default_cost_model(Some(CostModelKind::Full(CacheModel::L2Hit)));
	Engine::new(&config).expect("Failed to initialize PolkaVM.")
});

/// Process-global cache of compiled modules keyed by `keccak_256(program)`.
///
/// Held across runtime calls so `compile_from_hash` can reuse modules compiled by a
/// previous [`VirtManager`] instance. Stores [`Arc<CompiledModule>`] so the precomputed
/// export/import tables are shared across all reuses and cloning is O(1).
static MODULE_CACHE: LazyLock<RwLock<HashMap<[u8; 32], Arc<CompiledModule>>>> =
	LazyLock::new(|| RwLock::new(HashMap::new()));

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
				let bytes_slice = symbol.as_bytes();
				if bytes_slice.len() > MAX_SYSCALL_SYMBOL_LEN {
					return Err(ModuleError::InvalidImage);
				}
				let mut bytes = [0u8; MAX_SYSCALL_SYMBOL_LEN];
				bytes[..bytes_slice.len()].copy_from_slice(bytes_slice);
				Ok(SyscallSymbol { bytes, len: bytes_slice.len() as u64 })
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
/// NOTE: Module dedup across runtime calls is handled by the process-global [`MODULE_CACHE`].
/// Eventually that will be replaced by PolkaVM's built-in on-disk persistent cache.
pub struct VirtManager {
	instances: HashMap<InstanceId, ManagedInstance>,
	modules: HashMap<ModuleId, Arc<CompiledModule>>,
	instance_counter: u32,
	module_counter: u32,
}

impl Default for VirtManager {
	fn default() -> Self {
		Self {
			instances: HashMap::new(),
			modules: HashMap::new(),
			instance_counter: 0,
			module_counter: 0,
		}
	}
}

impl VirtManager {
	pub fn compile_from_bytes(&mut self, program: &[u8]) -> Result<ModuleId, ModuleError> {
		let mut module_config = ModuleConfig::new();
		module_config.set_gas_metering(Some(GasMeteringKind::Sync));
		let module =
			Module::new(&ENGINE, &module_config, program.into()).map_err(|err| match err {
				CompileError::ValidationFailed(err) => {
					log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
					ModuleError::InvalidImage
				},
				CompileError::Error(err) => {
					panic!("Polkavm failed during compilation: {err}. This is a bug.");
				},
			})?;
		let compiled = Arc::new(CompiledModule::new(module)?);

		let module_id = ModuleId({
			let old = self.module_counter;
			self.module_counter = old + 1;
			old
		});

		// Populate the process-global cache so subsequent `compile_from_hash` calls — possibly
		// from a different `VirtManager` instance in a later runtime call — can skip recompiling.
		// NOTE: keccak256 is chosen because pallet-revive uses it to identify code.
		// Eventually the hash function needs to be agreed upon with the PVM caching system.
		let hash = sp_crypto_hashing::keccak_256(program);
		MODULE_CACHE.write().unwrap().insert(hash, compiled.clone());
		self.modules.insert(module_id, compiled);

		Ok(module_id)
	}

	pub fn compile_from_hash(&mut self, hash: &[u8]) -> Result<ModuleId, ModuleError> {
		let hash: [u8; 32] = hash.try_into().map_err(|_| ModuleError::NotCached)?;
		let compiled =
			MODULE_CACHE.read().unwrap().get(&hash).cloned().ok_or(ModuleError::NotCached)?;
		let module_id = ModuleId({
			let old = self.module_counter;
			self.module_counter = old + 1;
			old
		});
		self.modules.insert(module_id, compiled);
		Ok(module_id)
	}

	pub fn instantiate(&mut self, module_id: ModuleId) -> Result<InstanceId, InstantiateError> {
		let compiled = self.modules.get(&module_id).ok_or(InstantiateError::InvalidModule)?.clone();

		let instance = compiled.module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?;

		let instance_id = InstanceId({
			let old = self.instance_counter;
			self.instance_counter = old + 1;
			old
		});

		self.instances.insert(
			instance_id,
			ManagedInstance { state: InstanceState::Idle(instance), module: compiled },
		);

		Ok(instance_id)
	}

	pub fn prepare(&mut self, instance_id: InstanceId, function: &[u8]) -> Result<(), ExecError> {
		let managed = self.instances.remove(&instance_id).ok_or(ExecError::InvalidInstance)?;
		let (managed, result) = Self::prepare_impl(managed, function);
		self.instances.insert(instance_id, managed);
		result
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

	pub fn run(
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

	pub fn destroy(&mut self, instance_id: InstanceId) -> Result<(), DestroyError> {
		if self.instances.remove(&instance_id).is_some() {
			Ok(())
		} else {
			Err(DestroyError::InvalidInstance)
		}
	}

	pub fn read_memory(
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

	pub fn write_memory(
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

sp_externalities::decl_extension! {
	/// Extension wrapping [`VirtManager`] so it can be accessed through
	/// the externalities by the virtualization host functions.
	pub struct VirtManagerExt(VirtManager);
}

impl Default for VirtManagerExt {
	fn default() -> Self {
		Self(VirtManager::default())
	}
}
