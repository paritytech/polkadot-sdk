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
	CallError, Caller, Config, Engine, GasMeteringKind, Instance, Linker, Module, ModuleConfig,
	RawInstance, Reg,
};
use std::{
	cell::RefCell,
	mem,
	rc::{Rc, Weak},
	sync::OnceLock,
};

/// User error type returned from `on_ecall` to signal that execution should stop.
///
/// This is used as the `UserError` type parameter for [`Instance`] and [`Linker`].
/// When the syscall handler sets `exit` to true, `on_ecall` returns this error which
/// propagates as `CallError::User(EcallError)`.
#[derive(Debug)]
struct EcallError;

impl core::fmt::Display for EcallError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "ecall requested exit")
	}
}

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
	/// The PolkaVM instance we are managing.
	instance: Instance<Self, EcallError>,
	/// Reference counted memory so that we can hand out multiple references to it.
	///
	/// This is needed because we need to make changes to the type from within `on_ecall`
	/// while we already handed out references to it.
	memory: Rc<RefCell<Memory>>,
	/// The fields which are only set while being within [`Self::execute`].
	while_exec: Option<WhileExec>,
}

/// Those are fields which are only set while [`Virt::execute`] is running.
///
/// Those types have their type parameter deleted because `on_ecall` can't be generic as a free
/// standing function without requiring `T` to be `'static`. Since we do not actually need
/// to access `T` in `on_ecall` we opt for deleting the type parameter instead.
struct WhileExec {
	/// The handler function that is called for every host function made by the program.
	///
	/// Transmuted from `SyscallHandler<T>` passed to [`Virt::execute`].
	syscall_handler: ErasedSyscallHandler,
	/// A pointer to the state that is shared between the syscall handler and us.
	///
	/// Represents `&mut SharedState<T>` passed to [`Virt::execute`]. We casted it into
	/// a raw pointer.
	state: usize,
}

/// The native [`MemoryT`] implementation.
///
/// Provides access to the guest memory of a [`RawInstance`] owned by [`Virt`].
///
/// This type exists because memory access is needed both from outside execution (through
/// the [`Instance`] in [`Virt`]) and from inside `on_ecall` callbacks (through the
/// [`Caller`]'s `&mut RawInstance`). Since [`Instance`] derefs to [`RawInstance`], both
/// paths reach the same underlying object. We share it via [`Rc`] so that [`MemoryT`]
/// consumers hold a [`Weak`] reference that becomes invalid when [`Virt`] is dropped.
///
/// # Safety
///
/// The inner pointer is valid for the lifetime of the [`Instance`] in [`Virt`]. All access
/// is confined to `read_memory_into` and `write_memory` on the [`RawInstance`], which do
/// not interfere with PolkaVM's execution state.
pub struct Memory(*mut RawInstance);

impl Memory {
	/// Create a new `Memory` from a mutable reference to an [`Instance`].
	///
	/// # Safety
	///
	/// The caller must ensure the [`Instance`] outlives all uses of this `Memory`.
	unsafe fn new(instance: &mut Instance<Virt, EcallError>) -> Self {
		Memory(&mut **instance as *mut RawInstance)
	}

	fn read(&self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		// SAFETY: The pointer is valid for the lifetime of `Virt` which owns
		// both the `Instance` and the `Rc<RefCell<Memory>>`.
		unsafe { (*self.0).read_memory_into(offset, dest) }
			.map(|_| ())
			.map_err(|_| MemoryError::OutOfBounds)
	}

	fn write(&self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		// SAFETY: The pointer is valid for the lifetime of `Virt` which owns
		// both the `Instance` and the `Rc<RefCell<Memory>>`.
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
	type Memory = Weak<RefCell<Memory>>;

	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError> {
		let engine = engine();

		let mut module_config = ModuleConfig::new();
		module_config.set_gas_metering(Some(GasMeteringKind::Sync));
		let module = Module::new(&engine, &module_config, program.into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
			InstantiateError::InvalidImage
		})?;

		let mut linker = Linker::<Self, EcallError>::new();
		linker.define_fallback(on_ecall);
		let instance = linker.instantiate_pre(&module).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to link program: {err}");
			InstantiateError::InvalidImage
		})?;

		let mut instance = instance.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?;
		// SAFETY: `instance` is moved into `Virt` below and lives as long as `Virt`,
		// which also owns the `Rc<RefCell<Memory>>`. The `Memory` is only accessed via
		// `Weak` references which become invalid once `Virt` is dropped.
		let memory = unsafe { Memory::new(&mut instance) };
		let virt = Self { while_exec: None, memory: Rc::new(RefCell::new(memory)), instance };
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

impl MemoryT for Weak<RefCell<Memory>> {
	fn read(&self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		let rc = self.upgrade().ok_or(MemoryError::InvalidInstance)?;
		let memory = rc.borrow();
		memory.read(offset, dest)
	}

	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		let rc = self.upgrade().ok_or(MemoryError::InvalidInstance)?;
		let memory = rc.borrow();
		memory.write(offset, src)
	}
}

impl Virt {
	/// Return a mutable reference to the state shared with the syscall handler.
	///
	/// # SAFETY
	///
	/// The caller must make sure that no other reference to [`Self::state`] exists
	/// while holding the reference returned from this function.
	///
	/// # Traps
	///
	/// Traps if being called outside of `on_ecall`.
	unsafe fn state(&mut self) -> &mut SharedState<()> {
		// # SAFETY
		//
		// ## Life Times
		//
		// The reference is created from a raw pointer which was in turn created from a
		// mutable reference passed into [`Self::`execute`]. This makes sure that no other
		// reference exists while inside `execute`. The pointer is stored within
		// [`Self::while_exec`] which is only set while being within `execute`.
		//
		// ## Change of generic parameter
		//
		// We transmute `&mut SharedState<T>` to `&mut SharedState<()>` here. This is safe because
		// `SharedState` is using #[repr(C)] alignment where the change of the last field will
		// not impact the alignment of the rest of the fields. Additionally, by choosing a ZST
		// for `T` we prevent any code that accesses this data from being generated. Hence
		// no assumptions over `T` will be made.
		&mut *(self
			.while_exec
			.as_mut()
			.expect(
				"Is set while executing. This function is only called from on_ecall;
				on_ecall is only called while executing; qed",
			)
			.state as *mut _)
	}

	fn internal_execute<T>(
		&mut self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError> {
		self.instance.reset_memory().map_err(|_| ExecError::InvalidInstance)?;
		self.instance.set_gas(state.gas_left);

		// It does not really make sense to set `exit` to true before calling execute. However,
		// it seems least surprising to not even start the execution in this case.
		if state.exit {
			return Ok(());
		}

		self.while_exec = Some(WhileExec {
			syscall_handler: syscall_handler.into(),
			state: state as *mut _ as usize,
		});
		// SAFETY: We need to pass `&mut self` (as user_data) to `call_typed` while also
		// calling it on `self.instance`. Rust's borrow checker cannot split borrows through
		// `self`, so we use a raw pointer. This is safe because `call_typed` only accesses
		// `user_data` from within `on_ecall` callbacks, which in turn only access
		// `self.while_exec` and `self.memory` — never `self.instance`.
		let virt_ptr = self as *mut Self;
		let outcome =
			self.instance
				.call_typed(unsafe { &mut *virt_ptr }, function, ())
				.map_err(|err| match err {
					CallError::Trap => ExecError::Trap,
					CallError::NotEnoughGas => ExecError::OutOfGas,
					CallError::Error(err) => {
						log::error!(target: LOG_TARGET, "polkavm execution error: {}", err);
						ExecError::InvalidImage
					},
					CallError::User(_) => ExecError::Trap,
					CallError::Step => ExecError::Trap,
				});

		self.while_exec = None;
		state.gas_left = self.instance.gas();

		outcome
	}
}

fn on_ecall(caller: Caller<'_, Virt>, syscall_id: u32) -> Result<(), EcallError> {
	let virt = caller.user_data;
	let instance = caller.instance;

	let a0 = instance.reg(Reg::A0) as u32;
	let a1 = instance.reg(Reg::A1) as u32;
	let a2 = instance.reg(Reg::A2) as u32;
	let a3 = instance.reg(Reg::A3) as u32;
	let a4 = instance.reg(Reg::A4) as u32;
	let a5 = instance.reg(Reg::A5) as u32;

	// make gas_left available to the syscall handler
	let gas_left_before = instance.gas();
	// SAFETY: no other reference is created from `state` while borrowing via
	// `state()`.
	unsafe {
		virt.state().gas_left = gas_left_before;
	}

	let while_exec = virt
		.while_exec
		.as_ref()
		.expect("Is set while executing. `on_ecall` is only called while executing; qed");

	// delegate to our syscall handler
	let result =
		(while_exec.syscall_handler.0)(while_exec.state, syscall_id, a0, a1, a2, a3, a4, a5);

	// SAFETY: no other reference is created from `state` while borrowing via
	// `state()`.
	let state = unsafe { virt.state() };

	// syscall handler might have reduced the gas left value
	let consumed = gas_left_before.saturating_sub(state.gas_left);
	instance.set_gas(instance.gas().saturating_sub(consumed));

	if state.exit {
		Err(EcallError)
	} else {
		instance.set_reg(Reg::A0, (result as u32).into());
		instance.set_reg(Reg::A1, ((result >> 32) as u32).into());
		Ok(())
	}
}
