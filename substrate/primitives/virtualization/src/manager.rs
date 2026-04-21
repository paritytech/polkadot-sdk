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
	sync::{LazyLock, RwLock},
};

/// This is the single PolkaVM engine we use for everything.
///
/// By using a common engine we allow PolkaVM to use caching. This caching is important
/// to reduce startup costs. This is even the case when instances use different code.
static ENGINE: LazyLock<Engine> = LazyLock::new(|| {
	let mut config = Config::from_env().expect("Invalid config.");
	config.set_worker_count(10);
	config.set_default_cost_model(Some(CostModelKind::Full(CacheModel::L1Hit)));
	Engine::new(&config).expect("Failed to initialize PolkaVM.")
});

/// Process-global cache of compiled modules keyed by `keccak_256(program)`.
///
/// Held across runtime calls so `compile_from_hash` can reuse modules compiled by a
/// previous [`VirtManager`] instance. Stores `Module` directly — it is internally an
/// `Arc` and cheap to clone.
static MODULE_CACHE: LazyLock<RwLock<HashMap<[u8; 32], Module>>> =
	LazyLock::new(|| RwLock::new(HashMap::new()));

fn map_memory_error(error: MemoryAccessError) -> MemoryError {
	match error {
		MemoryAccessError::OutOfRangeAccess { .. } | MemoryAccessError::MemoryLimitReached => {
			MemoryError::OutOfBounds
		},
		_ => {
			panic!("Error accessing polkavm memory. This is a bug.");
		},
	}
}

/// The state an instance can be in.
enum InstanceState {
	/// Idle — ready to be prepared for execution.
	Idle(RawInstance),
	/// Running — prepared and executing (possibly suspended at a syscall).
	Running(RawInstance),
}

/// Manages virtualization instances and their lifecycle.
///
/// Instance and module IDs are assigned deterministically from incrementing counters,
/// ensuring no non-determinism across different executions.
///
/// NOTE: Module dedup across runtime calls is handled by the process-global [`MODULE_CACHE`].
/// Eventually that will be replaced by PolkaVM's built-in on-disk persistent cache.
pub struct VirtManager {
	instances: HashMap<InstanceId, InstanceState>,
	modules: HashMap<ModuleId, Module>,
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
		MODULE_CACHE.write().unwrap().insert(hash, module.clone());
		self.modules.insert(module_id, module);

		Ok(module_id)
	}

	pub fn compile_from_hash(&mut self, hash: &[u8]) -> Result<ModuleId, ModuleError> {
		let hash: [u8; 32] = hash.try_into().map_err(|_| ModuleError::NotCached)?;
		let module =
			MODULE_CACHE.read().unwrap().get(&hash).cloned().ok_or(ModuleError::NotCached)?;
		let module_id = ModuleId({
			let old = self.module_counter;
			self.module_counter = old + 1;
			old
		});
		self.modules.insert(module_id, module);
		Ok(module_id)
	}

	pub fn instantiate(&mut self, module_id: ModuleId) -> Result<InstanceId, InstantiateError> {
		let module = self.modules.get(&module_id).ok_or(InstantiateError::InvalidModule)?;

		let instance = module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?;

		let instance_id = InstanceId({
			let old = self.instance_counter;
			self.instance_counter = old + 1;
			old
		});

		self.instances.insert(instance_id, InstanceState::Idle(instance));

		Ok(instance_id)
	}

	pub fn prepare(&mut self, instance_id: InstanceId, function: &[u8]) -> Result<(), ExecError> {
		let state = self.instances.remove(&instance_id).ok_or(ExecError::InvalidInstance)?;
		let (state, result) = Self::prepare_impl(state, function);
		self.instances.insert(instance_id, state);
		result
	}

	fn prepare_impl(
		state: InstanceState,
		function: &[u8],
	) -> (InstanceState, Result<(), ExecError>) {
		fn find_export(
			instance: &RawInstance,
			function: &[u8],
		) -> Result<ProgramCounter, ExecError> {
			instance
				.module()
				.exports()
				.find(|export| export.symbol().as_bytes() == function)
				.map(|export| export.program_counter())
				.ok_or_else(|| {
					log::debug!(
						target: LOG_TARGET,
						"Export not found: {}",
						String::from_utf8_lossy(function),
					);
					ExecError::InvalidImage
				})
		}

		let mut instance = match state {
			InstanceState::Idle(i) => i,
			running @ InstanceState::Running(_) => {
				return (running, Err(ExecError::InvalidInstance));
			},
		};
		match find_export(&instance, function) {
			Ok(pc) => {
				instance.prepare_call_untyped(pc, &[]);
				(InstanceState::Running(instance), Ok(()))
			},
			Err(err) => (InstanceState::Idle(instance), Err(err)),
		}
	}

	pub fn run(
		&mut self,
		instance_id: InstanceId,
		gas_left: i64,
		a0: u64,
	) -> Result<(ExecStatus, ExecBuffer), ExecError> {
		let state = self.instances.remove(&instance_id).ok_or(ExecError::InvalidInstance)?;
		let (state, result) = Self::run_impl(state, gas_left, a0);
		self.instances.insert(instance_id, state);
		result
	}

	fn run_impl(
		state: InstanceState,
		gas_left: i64,
		a0: u64,
	) -> (InstanceState, Result<(ExecStatus, ExecBuffer), ExecError>) {
		let mut instance = match state {
			InstanceState::Running(i) => i,
			idle @ InstanceState::Idle(_) => return (idle, Err(ExecError::InvalidInstance)),
		};

		instance.set_reg(Reg::A0, a0);
		instance.set_gas(gas_left);

		let interrupt = match instance.run() {
			Ok(interrupt) => interrupt,
			Err(err) => {
				log::error!(target: LOG_TARGET, "polkavm execution error: {}", err);
				return (InstanceState::Idle(instance), Err(ExecError::InvalidImage));
			},
		};

		match interrupt {
			InterruptKind::Finished => {
				let gas_left = instance.gas();
				(
					InstanceState::Idle(instance),
					Ok((ExecStatus::Finished, ExecBuffer { gas_left, ..Default::default() })),
				)
			},
			InterruptKind::Trap => (InstanceState::Idle(instance), Err(ExecError::Trap)),
			InterruptKind::NotEnoughGas => {
				(InstanceState::Idle(instance), Err(ExecError::OutOfGas))
			},
			InterruptKind::Step | InterruptKind::Segfault(_) => {
				unreachable!("PolkaVM failed. This is a bug.");
			},
			InterruptKind::Ecalli(hostcall_index) => {
				let Some(import_symbol) = instance
					.module()
					.imports()
					.get(hostcall_index)
					.filter(|s| s.as_bytes().len() <= MAX_SYSCALL_SYMBOL_LEN)
				else {
					return (InstanceState::Idle(instance), Err(ExecError::InvalidImage));
				};
				let import_symbol = import_symbol.as_bytes();
				let mut bytes = [0u8; MAX_SYSCALL_SYMBOL_LEN];
				bytes[..import_symbol.len()].copy_from_slice(import_symbol);
				let syscall_symbol = SyscallSymbol { bytes, len: import_symbol.len() as u64 };
				let gas_left = instance.gas();
				let a0 = instance.reg(Reg::A0);
				let a1 = instance.reg(Reg::A1);
				let a2 = instance.reg(Reg::A2);
				let a3 = instance.reg(Reg::A3);
				let a4 = instance.reg(Reg::A4);
				let a5 = instance.reg(Reg::A5);
				(
					InstanceState::Running(instance),
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
		let Some(InstanceState::Running(instance)) = self.instances.get_mut(&instance_id) else {
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
		let Some(InstanceState::Running(instance)) = self.instances.get_mut(&instance_id) else {
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
