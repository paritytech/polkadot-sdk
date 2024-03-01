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
	Caller, CallerRef, Config, Engine, ExecutionError, GasMeteringKind, Instance, Linker, Module,
	ModuleConfig, Reg, StateArgs, Trap,
};
use std::{
	cell::RefCell,
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
	/// The PolkaVM instance we are managing.
	instance: Instance<Self>,
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
pub enum Memory {
	/// While not executing we access memory through a [`Instance`].
	Idle(Instance<Virt>),
	/// While executing we have to use the [`Caller`] passed to `on_ecall` to access memory.
	///
	/// Trying to use an `Instance` while already executing it will lead to a dead lock. `on_ecall`
	/// will make sure to replace `Idle` with `Executing` before callign the syscall handler.
	Executing(CallerRef<()>),
}

impl Memory {
	fn into_caller(self) -> Option<CallerRef<()>> {
		match self {
			Self::Executing(caller) => Some(caller),
			_ => None,
		}
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
		module_config.set_gas_metering(Some(GasMeteringKind::Async));
		let module = Module::new(&engine, &module_config, program).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to compile program: {}", err);
			InstantiateError::InvalidImage
		})?;

		let mut linker = Linker::new(&engine);
		linker.func_fallback(on_ecall);
		let instance = linker.instantiate_pre(&module).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to link program: {err}");
			InstantiateError::InvalidImage
		})?;

		let instance = instance.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to instantiate program: {err}");
			InstantiateError::InvalidImage
		})?;
		let virt = Self {
			while_exec: None,
			memory: Rc::new(RefCell::new(Memory::Idle(instance.clone()))),
			instance,
		};
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
		let result = match &*rc.borrow() {
			Memory::Idle(instance) => instance.read_memory_into_slice(offset, dest),
			Memory::Executing(caller) => caller.read_memory_into_slice(offset, dest),
		};
		result.map(|_| ()).map_err(|_| MemoryError::OutOfBounds)
	}

	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		let rc = self.upgrade().ok_or(MemoryError::InvalidInstance)?;
		let result = match &mut *rc.borrow_mut() {
			Memory::Idle(instance) => instance.write_memory(offset, src),
			Memory::Executing(caller) => caller.write_memory(offset, src),
		};
		result.map_err(|_| MemoryError::OutOfBounds)
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
		let mut state_args = StateArgs::new();
		state_args.reset_memory(false).set_gas(state.gas_left.try_into().map_err(|_| {
			log::debug!(target: LOG_TARGET, "{} is not a valid gas value", state.gas_left);
			ExecError::InvalidGasValue
		})?);
		self.instance
			.update_state(state_args)
			.expect("We only set valid state above; qed");

		// It does not really make sense to set `exit` to true before calling execute. However,
		// it seems least surprising to not even start the execution in this case.
		if state.exit {
			return Ok(())
		}

		self.while_exec = Some(WhileExec {
			syscall_handler: syscall_handler.into(),
			state: state as *mut _ as usize,
		});
		let outcome =
			self.instance.clone().call_typed(self, function, ()).map_err(|err| match err {
				ExecutionError::Trap(_) => ExecError::Trap,
				ExecutionError::OutOfGas => ExecError::OutOfGas,
				ExecutionError::Error(err) => {
					log::error!(target: LOG_TARGET, "polkavm execution error: {}", err);
					ExecError::InvalidImage
				},
			});

		self.while_exec = None;
		state.gas_left = self.instance.gas_remaining().expect("metering is enabled; qed").get();

		outcome
	}
}

fn on_ecall(caller: Caller<'_, Virt>, syscall_id: &[u8]) -> Result<(), Trap> {
	let syscall_no = if syscall_id.len() == 4 {
		u32::from_le_bytes([syscall_id[0], syscall_id[1], syscall_id[2], syscall_id[3]])
	} else {
		log::debug!(
			target: LOG_TARGET,
			"All syscall identifiers need to be exactly 4 bytes. Supplied id: {:?}",
			syscall_id
		);
		return Err(Trap::default());
	};

	let (caller, virt) = caller.split();

	// caller is moved later and hence we need to copy the register values
	let a0 = caller.get_reg(Reg::A0);
	let a1 = caller.get_reg(Reg::A1);
	let a2 = caller.get_reg(Reg::A2);
	let a3 = caller.get_reg(Reg::A3);
	let a4 = caller.get_reg(Reg::A4);
	let a5 = caller.get_reg(Reg::A5);

	// make gas_left available to the syscall handler
	let gas_left_before = caller.gas_remaining().expect("metering is enabled; qed").get();
	// SAFETY: no other reference is created from `state` while borrowing via
	// `state()`.
	unsafe {
		virt.state().gas_left = gas_left_before;
	}

	let instance =
		mem::replace(&mut *virt.memory.borrow_mut(), Memory::Executing(caller.into_ref()));

	let while_exec = virt
		.while_exec
		.as_ref()
		.expect("Is set while executing. `on_ecall` is only called while executing; qed");

	// delegate to our syscall handler
	let result =
		(while_exec.syscall_handler.0)(while_exec.state, syscall_no, a0, a1, a2, a3, a4, a5);

	let mut caller = mem::replace(&mut *virt.memory.borrow_mut(), instance).into_caller().expect(
		"We just set this to a a caller before calling into the syscaller handler.
		The syscall handler cannot change this field; qed",
	);

	// SAFETY: no other reference is created from `state` while borrowing via
	// `state()`.
	let state = unsafe { virt.state() };

	// syscall handler might have reduced the gas left value
	let consumed = gas_left_before.saturating_sub(state.gas_left);
	caller.consume_gas(consumed);

	if state.exit {
		Err(Trap::default())
	} else {
		caller.set_reg(Reg::A0, result as u32);
		caller.set_reg(Reg::A1, (result >> 32) as u32);
		Ok(())
	}
}
