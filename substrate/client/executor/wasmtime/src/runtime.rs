// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Defines the compiled Wasm runtime that uses Wasmtime internally.

use crate::host::HostState;
use crate::imports::{resolve_imports, Imports};
use crate::instance_wrapper::InstanceWrapper;
use crate::state_holder::StateHolder;

use sc_executor_common::{
	error::{Error, Result, WasmError},
	wasm_runtime::WasmRuntime,
};
use sp_allocator::FreeingBumpHeapAllocator;
use sp_runtime_interface::unpack_ptr_and_len;
use sp_wasm_interface::{Function, Pointer, WordSize, Value};
use wasmtime::{Config, Engine, Module, Store};

/// A `WasmRuntime` implementation using wasmtime to compile the runtime module to machine code
/// and execute the compiled code.
pub struct WasmtimeRuntime {
	module: Module,
	imports: Imports,
	state_holder: StateHolder,
	heap_pages: u32,
	host_functions: Vec<&'static dyn Function>,
}

impl WasmRuntime for WasmtimeRuntime {
	fn host_functions(&self) -> &[&'static dyn Function] {
		&self.host_functions
	}

	fn call(&mut self, method: &str, data: &[u8]) -> Result<Vec<u8>> {
		call_method(
			&self.module,
			&mut self.imports,
			&self.state_holder,
			method,
			data,
			self.heap_pages,
		)
	}

	fn get_global_val(&self, name: &str) -> Result<Option<Value>> {
		// Yeah, there is no better way currently :(
		InstanceWrapper::new(&self.module, &self.imports, self.heap_pages)?
			.get_global_val(name)
	}
}

/// Create a new `WasmtimeRuntime` given the code. This function performs translation from Wasm to
/// machine code, which can be computationally heavy.
pub fn create_instance(
	code: &[u8],
	heap_pages: u64,
	host_functions: Vec<&'static dyn Function>,
	allow_missing_func_imports: bool,
) -> std::result::Result<WasmtimeRuntime, WasmError> {
	// Create the engine, store and finally the module from the given code.
	let mut config = Config::new();
	config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);

	let engine = Engine::new(&config);
	let store = Store::new(&engine);
	let module = Module::new(&store, code)
		.map_err(|e| WasmError::Other(format!("cannot create module: {}", e)))?;

	let state_holder = StateHolder::empty();

	// Scan all imports, find the matching host functions, and create stubs that adapt arguments
	// and results.
	let imports = resolve_imports(
		&state_holder,
		&module,
		&host_functions,
		heap_pages as u32,
		allow_missing_func_imports,
	)?;

	Ok(WasmtimeRuntime {
		module,
		imports,
		state_holder,
		heap_pages: heap_pages as u32,
		host_functions,
	})
}

/// Call a function inside a precompiled Wasm module.
fn call_method(
	module: &Module,
	imports: &mut Imports,
	state_holder: &StateHolder,
	method: &str,
	data: &[u8],
	heap_pages: u32,
) -> Result<Vec<u8>> {
	let instance_wrapper = InstanceWrapper::new(module, imports, heap_pages)?;
	let entrypoint = instance_wrapper.resolve_entrypoint(method)?;
	let heap_base = instance_wrapper.extract_heap_base()?;
	let allocator = FreeingBumpHeapAllocator::new(heap_base);

	perform_call(data, state_holder, instance_wrapper, entrypoint, allocator)
}

fn perform_call(
	data: &[u8],
	state_holder: &StateHolder,
	instance_wrapper: InstanceWrapper,
	entrypoint: wasmtime::Func,
	mut allocator: FreeingBumpHeapAllocator,
) -> Result<Vec<u8>> {
	let (data_ptr, data_len) = inject_input_data(&instance_wrapper, &mut allocator, data)?;

	let host_state = HostState::new(allocator, instance_wrapper);
	let (ret, host_state) = state_holder.with_initialized_state(host_state, || {
		match entrypoint.call(&[
			wasmtime::Val::I32(u32::from(data_ptr) as i32),
			wasmtime::Val::I32(u32::from(data_len) as i32),
		]) {
			Ok(results) => {
				let retval = results[0].unwrap_i64() as u64;
				Ok(unpack_ptr_and_len(retval))
			}
			Err(trap) => {
				return Err(Error::from(format!(
					"Wasm execution trapped: {}",
					trap.message()
				)));
			}
		}
	});
	let (output_ptr, output_len) = ret?;

	let instance = host_state.into_instance();
	let output = extract_output_data(&instance, output_ptr, output_len)?;

	Ok(output)
}

fn inject_input_data(
	instance: &InstanceWrapper,
	allocator: &mut FreeingBumpHeapAllocator,
	data: &[u8],
) -> Result<(Pointer<u8>, WordSize)> {
	let data_len = data.len() as WordSize;
	let data_ptr = instance.allocate(allocator, data_len)?;
	instance.write_memory_from(data_ptr, data)?;
	Ok((data_ptr, data_len))
}

fn extract_output_data(
	instance: &InstanceWrapper,
	output_ptr: u32,
	output_len: u32,
) -> Result<Vec<u8>> {
	let mut output = vec![0; output_len as usize];
	instance.read_memory_into(Pointer::new(output_ptr), &mut output)?;
	Ok(output)
}
