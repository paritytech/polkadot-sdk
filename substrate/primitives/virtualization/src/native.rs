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
	ExecError, InstantiateError, MemoryError, MemoryT, SharedState, SyscallHandler, VirtT,
	LOG_TARGET,
};
use polkavm::{
	Config, Engine, GasMeteringKind, InterruptKind, Module, ModuleConfig, ProgramCounter,
	RawInstance, Reg,
};
use std::{
	mem,
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
	/// The PolkaVM raw instance we are managing.
	///
	/// Boxed so that its heap address is stable across moves of `Virt`. This allows
	/// [`Memory`] to hold a raw pointer to the inner [`RawInstance`] that remains valid
	/// for the lifetime of this struct.
	instance: Box<RawInstance>,
	/// The compiled module, kept around so we can resolve import indices to symbols.
	module: Module,
	/// Shared memory handle handed out via [`VirtT::memory`].
	///
	/// Users hold [`Weak`] references; once [`Virt`] is dropped the [`Rc`] is the last
	/// owner and all [`Weak`] references become invalid.
	memory: Rc<Memory>,
}

/// The native [`MemoryT`] implementation.
///
/// Provides access to the guest memory of a [`RawInstance`] owned by [`Virt`].
///
/// # Safety
///
/// The inner `*mut RawInstance` is derived from `Box<RawInstance>` which is heap-allocated
/// and never moves. The pointer is valid for the lifetime of [`Virt`] which owns both the
/// [`Box`] and the [`Rc<Memory>`]. Users only hold [`Weak`] references that become invalid
/// once [`Virt`] is dropped.
pub struct Memory(*mut RawInstance);

impl Memory {
	fn read(&self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		// SAFETY: See `Memory` doc comment. The pointer is valid for the lifetime of `Virt`.
		unsafe { (*self.0).read_memory_into(offset, dest) }
			.map(|_| ())
			.map_err(|_| MemoryError::OutOfBounds)
	}

	fn write(&self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		// SAFETY: See `Memory` doc comment. The pointer is valid for the lifetime of `Virt`.
		unsafe { (*self.0).write_memory(offset, src) }.map_err(|_| MemoryError::OutOfBounds)
	}
}

/// This is the none generic version of [`SyscallHandler`].
///
/// It is identical to [`SyscallHandler`] with the exception of the first parameter which
/// is replaced by a pointer. It is safe to transmute between the two because `usize` and
/// references are ABI compatible.
struct ErasedSyscallHandler(
	extern "C" fn(
		// &mut SharedState<T>
		state: usize,
		syscall_no: u32,
		a0: u32,
		a1: u32,
		a2: u32,
		a3: u32,
		a4: u32,
		a5: u32,
	) -> u64,
);

impl<T> From<SyscallHandler<T>> for ErasedSyscallHandler {
	fn from(from: SyscallHandler<T>) -> ErasedSyscallHandler {
		// SAFETY: `SyscallHandler` and `ErasedSyscallHandler` are ABI compatible
		unsafe { ErasedSyscallHandler(mem::transmute(from)) }
	}
}

impl VirtT for Virt {
	// We use a weak reference in order to be compatible to the forwarder implementation
	// where the memory is no longer accessible once the `Virt` is destroyed.
	type Memory = Weak<Memory>;

	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError> {
		let engine = engine();

		let mut module_config = ModuleConfig::new();
		module_config.set_gas_metering(Some(GasMeteringKind::Sync));
		let module = Module::new(&engine, &module_config, program.into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
			InstantiateError::InvalidImage
		})?;

		let mut instance = Box::new(module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?);
		// SAFETY: `instance` is `Box`-allocated so `&mut *instance` (the `RawInstance`)
		// has a stable heap address. The pointer remains valid for the lifetime of `Virt`
		// which owns both the `Box` and the `Rc<Memory>`.
		let ptr = &mut *instance as *mut RawInstance;
		let memory = Rc::new(Memory(ptr));
		let virt = Self { memory, instance, module };
		Ok(virt)
	}

	fn execute<T>(
		&mut self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError> {
		self.internal_execute(function, syscall_handler, state)
	}

	fn execute_and_destroy<T>(
		mut self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError> {
		self.internal_execute(function, syscall_handler, state)
	}

	fn memory(&self) -> Self::Memory {
		Rc::downgrade(&self.memory)
	}
}

impl MemoryT for Weak<Memory> {
	fn read(&self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		let memory = self.upgrade().ok_or(MemoryError::InvalidInstance)?;
		memory.read(offset, dest)
	}

	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		let memory = self.upgrade().ok_or(MemoryError::InvalidInstance)?;
		memory.write(offset, src)
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

	fn internal_execute<T>(
		&mut self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError> {
		self.instance.set_gas(state.gas_left);

		// It does not really make sense to set `exit` to true before calling execute. However,
		// it seems least surprising to not even start the execution in this case.
		if state.exit {
			return Ok(());
		}

		let pc = self.find_export(function)?;
		self.instance.prepare_call_typed(pc, ());

		let erased_handler: ErasedSyscallHandler = syscall_handler.into();
		// SAFETY: We transmute `&mut SharedState<T>` to `&mut SharedState<()>` below.
		// This is safe because `SharedState` uses `#[repr(C)]` where changing the last
		// field to a ZST doesn't affect alignment of the preceding fields. By choosing
		// `()` we prevent any code from accessing the user data through this pointer.
		let state_ptr = state as *mut SharedState<T> as usize;

		let outcome = loop {
			let interrupt = self.instance.run().map_err(|err| {
				log::error!(target: LOG_TARGET, "polkavm execution error: {}", err);
				ExecError::InvalidImage
			})?;

			match interrupt {
				InterruptKind::Finished => break Ok(()),
				InterruptKind::Trap => break Err(ExecError::Trap),
				InterruptKind::NotEnoughGas => break Err(ExecError::OutOfGas),
				InterruptKind::Step => break Err(ExecError::Trap),
				InterruptKind::Segfault(_) => break Err(ExecError::Trap),
				InterruptKind::Ecalli(hostcall_index) => {
					// The `hostcall_index` is an index into the module's import table,
					// not the actual syscall number. We need to resolve it by looking up
					// the symbol bytes.
					let syscall_symbol =
						self.module.imports().get(hostcall_index).expect(
							"hostcall index is valid because it was generated by polkavm; qed",
						);
					let syscall_id = u32::from_le_bytes(
						syscall_symbol
							.as_bytes()
							.try_into()
							.expect("syscall symbols are always 4 bytes; qed"),
					);

					let a0 = self.instance.reg(Reg::A0) as u32;
					let a1 = self.instance.reg(Reg::A1) as u32;
					let a2 = self.instance.reg(Reg::A2) as u32;
					let a3 = self.instance.reg(Reg::A3) as u32;
					let a4 = self.instance.reg(Reg::A4) as u32;
					let a5 = self.instance.reg(Reg::A5) as u32;

					// Make gas_left available to the syscall handler.
					let gas_left_before = self.instance.gas();
					// SAFETY: `state_ptr` points to a valid `SharedState<T>`, and we only
					// access the non-generic fields (gas_left, exit) through the `()`
					// version. No other reference to the state exists at this point.
					let state_ref = unsafe { &mut *(state_ptr as *mut SharedState<()>) };
					state_ref.gas_left = gas_left_before;

					// Delegate to the syscall handler.
					let result = (erased_handler.0)(state_ptr, syscall_id, a0, a1, a2, a3, a4, a5);

					// Re-read state after handler may have modified it.
					let state_ref = unsafe { &mut *(state_ptr as *mut SharedState<()>) };

					// Syscall handler might have reduced the gas left value.
					let consumed = gas_left_before.saturating_sub(state_ref.gas_left);
					self.instance.set_gas(self.instance.gas().saturating_sub(consumed));

					if state_ref.exit {
						break Err(ExecError::Trap);
					}

					self.instance.set_reg(Reg::A0, (result as u32).into());
					self.instance.set_reg(Reg::A1, ((result >> 32) as u32).into());
				},
			}
		};

		state.gas_left = self.instance.gas();
		outcome
	}
}
