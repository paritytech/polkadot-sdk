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

use crate::{
	ExecAction, ExecError, ExecOutcome, InstantiateError, MemoryError, MemoryT, VirtT, LOG_TARGET,
};
use polkavm::{
	Config, Engine, GasMeteringKind, InterruptKind, Module, ModuleConfig, ProgramCounter,
	RawInstance, Reg,
};
use std::{
	cell::RefCell,
	rc::{Rc, Weak},
	sync::OnceLock,
};

/// This is the single PolkaVM engine we use for everything.
///
/// By using a common engine we allow PolkaVM to use caching. This caching is important
/// to reduce startup costs. This is even the case when instances use different code.
static ENGINE: OnceLock<Engine> = OnceLock::new();

/// Engine wide configuration.
fn engine() -> &'static Engine {
	ENGINE.get_or_init(|| {
		let config = Config::from_env().expect("Invalid config.");
		Engine::new(&config).expect("Failed to initialize PolkaVM.")
	})
}

/// Native implementation of [`VirtT`].
pub struct Virt {
	/// The PolkaVM raw instance behind shared ownership.
	///
	/// [`Memory`] handles hold [`Weak`] references to the same [`RefCell`].
	/// Once [`Virt`] is dropped the [`Rc`] is the last owner and all [`Weak`]
	/// references become invalid, causing memory operations to return
	/// [`MemoryError::InvalidInstance`].
	instance: Rc<RefCell<RawInstance>>,
	/// The compiled module, kept around so we can resolve import indices to symbols.
	module: Module,
	/// Whether the instance is in the middle of an execution (awaiting resume).
	executing: bool,
}

/// The native [`MemoryT`] implementation.
///
/// Provides access to the guest memory of a [`RawInstance`] owned by [`Virt`].
/// Holds a [`Weak`] reference so that memory access fails gracefully once the
/// owning [`Virt`] is dropped.
pub struct Memory(Weak<RefCell<RawInstance>>);

impl MemoryT for Memory {
	fn read(&mut self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		let instance = self.0.upgrade().ok_or(MemoryError::InvalidInstance)?;
		let mut guard = instance.borrow_mut();
		guard
			.read_memory_into(offset, dest)
			.map(|_| ())
			.map_err(|_| MemoryError::OutOfBounds)
	}

	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		let instance = self.0.upgrade().ok_or(MemoryError::InvalidInstance)?;
		let mut guard = instance.borrow_mut();
		guard.write_memory(offset, src).map_err(|_| MemoryError::OutOfBounds)
	}
}

impl VirtT for Virt {
	// We use a weak reference in order to be compatible to the forwarder implementation
	// where the memory is no longer accessible once the `Virt` is destroyed.
	type Memory = Memory;

	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError> {
		let engine = engine();

		let mut module_config = ModuleConfig::new();
		module_config.set_gas_metering(Some(GasMeteringKind::Sync));
		let module = Module::new(&engine, &module_config, program.into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
			InstantiateError::InvalidImage
		})?;

		let instance = Rc::new(RefCell::new(module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?));
		Ok(Self { instance, module, executing: false })
	}

	fn run(&mut self, gas_left: i64, action: ExecAction<'_>) -> Result<ExecOutcome, ExecError> {
		{
			let mut instance = self.instance.borrow_mut();
			match action {
				ExecAction::Execute(function) => {
					if self.executing {
						return Err(ExecError::InvalidInstance);
					}
					let pc = self.find_export(function)?;
					instance.prepare_call_typed(pc, ());
				},
				ExecAction::Resume(return_value) => {
					if !self.executing {
						return Err(ExecError::InvalidInstance);
					}
					instance.set_reg(Reg::A0, return_value);
				},
			}
			instance.set_gas(gas_left);
		}
		self.step()
	}

	fn memory(&self) -> Self::Memory {
		Memory(Rc::downgrade(&self.instance))
	}
}

impl Virt {
	fn find_export(&self, function: &str) -> Result<ProgramCounter, ExecError> {
		self.module
			.exports()
			.find(|export| export.symbol().as_bytes() == function.as_bytes())
			.map(|export| export.program_counter())
			.ok_or_else(|| {
				log::debug!(
					target: LOG_TARGET,
					"Export not found: {function}"
				);
				ExecError::InvalidImage
			})
	}

	/// Run the instance until the next interrupt and return the outcome.
	fn step(&mut self) -> Result<ExecOutcome, ExecError> {
		let mut instance = self.instance.borrow_mut();
		let interrupt = instance.run().map_err(|err| {
			self.executing = false;
			log::error!(target: LOG_TARGET, "polkavm execution error: {}", err);
			ExecError::InvalidImage
		})?;

		match interrupt {
			InterruptKind::Finished => {
				self.executing = false;
				Ok(ExecOutcome::Finished { gas_left: instance.gas() })
			},
			InterruptKind::Trap => {
				self.executing = false;
				Err(ExecError::Trap)
			},
			InterruptKind::NotEnoughGas => {
				self.executing = false;
				Err(ExecError::OutOfGas)
			},
			InterruptKind::Step | InterruptKind::Segfault(_) => {
				self.executing = false;
				Err(ExecError::Trap)
			},
			InterruptKind::Ecalli(hostcall_index) => {
				self.executing = true;
				// The `hostcall_index` is an index into the module's import table,
				// not the actual syscall number. We need to resolve it by looking up
				// the symbol bytes.
				let syscall_symbol = self
					.module
					.imports()
					.get(hostcall_index)
					.expect("hostcall index is valid because it was generated by polkavm; qed");
				let syscall_id = u32::from_le_bytes(
					syscall_symbol
						.as_bytes()
						.try_into()
						.expect("syscall symbols are always 4 bytes; qed"),
				);

				Ok(ExecOutcome::Syscall {
					gas_left: instance.gas(),
					syscall_no: syscall_id,
					a0: instance.reg(Reg::A0),
					a1: instance.reg(Reg::A1),
					a2: instance.reg(Reg::A2),
					a3: instance.reg(Reg::A3),
					a4: instance.reg(Reg::A4),
					a5: instance.reg(Reg::A5),
				})
			},
		}
	}
}
