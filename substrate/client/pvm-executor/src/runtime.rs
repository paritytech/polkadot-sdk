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

//! A compiled and linked JAM runtime blob.

use polkavm::{CallError, Caller, Reg};
use sc_executor_common::error::{Error, WasmError};
use sp_jam_runtime_interface::{HostFunction, MAX_ARGS};
use std::sync::OnceLock;

/// The largest call payload a runtime can be handed, in bytes.
///
/// The payload is written into the aux-data region, which the runtime's picoalloc allocator never
/// touches. A payload at the heap base would be overwritten by the first allocation.
const MAX_INPUT_SIZE: u32 = 64 * 1024 * 1024;

/// `jam_types::SimpleResultCode::Nothing`, the JAM `NONE` host-call result.
const SIMPLE_RESULT_NOTHING: u64 = u64::MAX;

/// The engine shared by all runtimes, configured from the `POLKAVM_*` environment variables.
fn engine() -> Result<&'static polkavm::Engine, WasmError> {
	static ENGINE: OnceLock<Result<polkavm::Engine, String>> = OnceLock::new();

	ENGINE
		.get_or_init(|| {
			let config = polkavm::Config::from_env().map_err(|e| e.to_string())?;
			polkavm::Engine::new(&config).map_err(|e| e.to_string())
		})
		.as_ref()
		.map_err(|e| WasmError::Other(e.clone()))
}

/// A compiled runtime blob with all its imports resolved.
pub(crate) struct Runtime {
	instance_pre: polkavm::InstancePre<(), String>,
}

impl Runtime {
	/// Compile `code` and link it against `host_functions`.
	pub(crate) fn new(
		code: &[u8],
		host_functions: Vec<&'static dyn HostFunction>,
	) -> Result<Self, WasmError> {
		let blob = polkavm::ProgramBlob::parse(code.into())
			.map_err(|e| WasmError::Other(format!("failed to parse the PolkaVM blob: {e}")))?;

		let mut module_config = polkavm::ModuleConfig::default();
		module_config.set_aux_data_size(MAX_INPUT_SIZE);

		let module = polkavm::Module::from_blob(engine()?, &module_config, blob)
			.map_err(|e| WasmError::Other(format!("failed to compile the PolkaVM blob: {e}")))?;

		let mut linker = polkavm::Linker::<(), String>::new();
		link(&mut linker, host_functions).map_err(|e| WasmError::Instantiation(e.to_string()))?;

		let instance_pre = linker
			.instantiate_pre(&module)
			.map_err(|e| WasmError::Instantiation(e.to_string()))?;

		Ok(Self { instance_pre })
	}

	/// Call the export `method` with `data` as input.
	pub(crate) fn call(&self, method: &str, data: &[u8]) -> Result<Vec<u8>, Error> {
		let failed = |err: &dyn std::fmt::Display| -> Error {
			format!("call into the runtime method '{method}' failed: {err}").into()
		};

		let mut instance = self.instance_pre.instantiate().map_err(|e| failed(&e))?;

		let data_len = u32::try_from(data.len()).map_err(|_| failed(&"input payload too big"))?;
		instance.set_accessible_aux_size(data_len).map_err(|e| {
			failed(&format!("input payload of {data_len} bytes does not fit the input region: {e}"))
		})?;
		let data_ptr = instance.module().memory_map().aux_data_address();
		instance.write_memory(data_ptr, data).map_err(|e| failed(&e))?;

		match instance.call_typed(&mut (), method, (data_ptr, data_len)) {
			Ok(()) => {},
			Err(CallError::Trap) => return Err(failed(&"trap")),
			Err(CallError::Error(err)) => return Err(failed(&err)),
			Err(CallError::User(err)) => return Err(failed(&err)),
			Err(CallError::NotEnoughGas) => unreachable!("gas metering is never enabled"),
			Err(CallError::Step) => unreachable!("stepping is never enabled"),
		}

		let result = instance.reg(Reg::A0);
		let (result_ptr, result_len) =
			((result & u64::from(u32::MAX)) as u32, (result >> 32) as u32);
		instance
			.read_memory(result_ptr, result_len)
			.map_err(|e| failed(&format!("failed to read the return payload: {e}")))
	}
}

/// Define `host_functions` and the built-in `grow_heap` and `fetch` imports on `linker`.
fn link(
	linker: &mut polkavm::Linker<(), String>,
	host_functions: Vec<&'static dyn HostFunction>,
) -> Result<(), polkavm::Error> {
	for function in host_functions {
		linker.define_untyped(function.name(), |mut caller: Caller<()>| {
			call_host_function(&mut caller, function)
		})?;
	}

	// `grow_heap` is imported directly by `jam-pvm-common`'s picoalloc. It takes the requested
	// absolute heap size in pages and returns the resulting heap size in pages; a result smaller
	// than requested signals failure.
	linker.define_untyped("grow_heap", |caller: Caller<()>| {
		let requested = caller.instance.reg(Reg::A0);
		let page_size = u64::from(caller.instance.module().memory_map().page_size());
		let top = match caller.instance.sbrk(0) {
			Ok(Some(top)) => u64::from(top),
			Ok(None) => {
				caller.instance.set_reg(Reg::A0, 0);
				return Ok(());
			},
			Err(e) => return Err(e.to_string()),
		};
		let top_pages = top.div_ceil(page_size);

		if requested <= top_pages {
			caller.instance.set_reg(Reg::A0, top_pages);
			return Ok(());
		}

		let delta = (requested - top_pages)
			.checked_mul(page_size)
			.and_then(|delta| u32::try_from(delta).ok());
		let Some(delta) = delta else {
			caller.instance.set_reg(Reg::A0, top_pages);
			return Ok(());
		};

		match caller.instance.sbrk(delta) {
			Ok(Some(_)) => caller.instance.set_reg(Reg::A0, requested),
			Ok(None) => caller.instance.set_reg(Reg::A0, top_pages),
			Err(e) => return Err(e.to_string()),
		}
		Ok(())
	})?;

	// `fetch` (Gray Paper index 2) is imported directly by `jam-pvm-common`. Only the PVF serves
	// fetch data; on the node every kind answers `NONE`.
	linker.define_untyped("fetch", |caller: Caller<()>| {
		caller.instance.set_reg(Reg::A0, SIMPLE_RESULT_NOTHING);
		Ok(())
	})?;

	Ok(())
}

/// The guest memory as seen by the node side of a host call.
struct Context<'r, 'a>(&'r mut Caller<'a, ()>);

impl sp_jam_runtime_interface::FunctionContext for Context<'_, '_> {
	fn read_memory_into(&mut self, ptr: *const u8, dest: &mut [u8]) -> Result<(), String> {
		self.0
			.instance
			.read_memory_into(ptr as u32, dest)
			.map(|_| ())
			.map_err(|error| error.to_string())
	}

	fn write_memory(&mut self, ptr: *mut u8, data: &[u8]) -> Result<(), String> {
		self.0
			.instance
			.write_memory(ptr as u32, data)
			.map_err(|error| error.to_string())
	}
}

struct Registers([u64; MAX_ARGS]);

impl sp_jam_runtime_interface::Registers for Registers {
	fn arg(&self, index: usize) -> u64 {
		self.0[index]
	}
}

/// Serve one host call: every argument and the return value is one register.
fn call_host_function(caller: &mut Caller<()>, function: &dyn HostFunction) -> Result<(), String> {
	let mut regs = [0u64; MAX_ARGS];
	for (reg, value) in Reg::ARG_REGS.iter().zip(regs.iter_mut()) {
		*value = caller.instance.reg(*reg);
	}

	log::trace!(target: crate::LOG_TARGET, "Calling host function: '{}', args = {regs:?}", function.name());

	let value = function.execute(&mut Context(caller), &Registers(regs)).map_err(|error| {
		format!("call into the host function '{}' failed: {error}", function.name())
	})?;

	if let Some(value) = value {
		caller.instance.set_reg(Reg::A0, value);
	}

	Ok(())
}
