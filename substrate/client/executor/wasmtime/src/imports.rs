// This file is part of Substrate.

// Copyright (C) 2020 Parity Technologies (UK) Ltd.
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

use crate::state_holder;
use sc_executor_common::error::WasmError;
use sp_wasm_interface::{Function, Value, ValueType};
use std::any::Any;
use wasmtime::{
	Extern, ExternType, Func, FuncType, ImportType, Limits, Memory, MemoryType, Module,
	Trap, Val,
};

pub struct Imports {
	/// Contains the index into `externs` where the memory import is stored if any. `None` if there
	/// is none.
	pub memory_import_index: Option<usize>,
	pub externs: Vec<Extern>,
}

/// Goes over all imports of a module and prepares a vector of `Extern`s that can be used for
/// instantiation of the module. Returns an error if there are imports that cannot be satisfied.
pub fn resolve_imports(
	module: &Module,
	host_functions: &[&'static dyn Function],
	heap_pages: u32,
	allow_missing_func_imports: bool,
) -> Result<Imports, WasmError> {
	let mut externs = vec![];
	let mut memory_import_index = None;
	for import_ty in module.imports() {
		if import_ty.module() != "env" {
			return Err(WasmError::Other(format!(
				"host doesn't provide any imports from non-env module: {}:{}",
				import_ty.module(),
				import_ty.name()
			)));
		}

		let resolved = match import_ty.name() {
			"memory" => {
				memory_import_index = Some(externs.len());
				resolve_memory_import(module, &import_ty, heap_pages)?
			}
			_ => resolve_func_import(
				module,
				&import_ty,
				host_functions,
				allow_missing_func_imports,
			)?,
		};
		externs.push(resolved);
	}
	Ok(Imports {
		memory_import_index,
		externs,
	})
}

fn resolve_memory_import(
	module: &Module,
	import_ty: &ImportType,
	heap_pages: u32,
) -> Result<Extern, WasmError> {
	let requested_memory_ty = match import_ty.ty() {
		ExternType::Memory(memory_ty) => memory_ty,
		_ => {
			return Err(WasmError::Other(format!(
				"this import must be of memory type: {}:{}",
				import_ty.module(),
				import_ty.name()
			)))
		}
	};

	// Increment the min (a.k.a initial) number of pages by `heap_pages` and check if it exceeds the
	// maximum specified by the import.
	let initial = requested_memory_ty
		.limits()
		.min()
		.saturating_add(heap_pages);
	if let Some(max) = requested_memory_ty.limits().max() {
		if initial > max {
			return Err(WasmError::Other(format!(
				"incremented number of pages by heap_pages (total={}) is more than maximum requested\
				by the runtime wasm module {}",
				initial,
				max,
			)));
		}
	}

	let memory_ty = MemoryType::new(Limits::new(initial, requested_memory_ty.limits().max()));
	let memory = Memory::new(module.store(), memory_ty);
	Ok(Extern::Memory(memory))
}

fn resolve_func_import(
	module: &Module,
	import_ty: &ImportType,
	host_functions: &[&'static dyn Function],
	allow_missing_func_imports: bool,
) -> Result<Extern, WasmError> {
	let func_ty = match import_ty.ty() {
		ExternType::Func(func_ty) => func_ty,
		_ => {
			return Err(WasmError::Other(format!(
				"host doesn't provide any non function imports besides 'memory': {}:{}",
				import_ty.module(),
				import_ty.name()
			)));
		}
	};

	let host_func = match host_functions
		.iter()
		.find(|host_func| host_func.name() == import_ty.name())
	{
		Some(host_func) => host_func,
		None if allow_missing_func_imports => {
			return Ok(MissingHostFuncHandler::new(import_ty).into_extern(module, &func_ty));
		}
		None => {
			return Err(WasmError::Other(format!(
				"host doesn't provide such function: {}:{}",
				import_ty.module(),
				import_ty.name()
			)));
		}
	};
	if !signature_matches(&func_ty, &wasmtime_func_sig(*host_func)) {
		return Err(WasmError::Other(format!(
			"signature mismatch for: {}:{}",
			import_ty.module(),
			import_ty.name()
		)));
	}

	Ok(HostFuncHandler::new(*host_func).into_extern(module))
}

/// Returns `true` if `lhs` and `rhs` represent the same signature.
fn signature_matches(lhs: &wasmtime::FuncType, rhs: &wasmtime::FuncType) -> bool {
	lhs.params() == rhs.params() && lhs.results() == rhs.results()
}

/// This structure implements `Callable` and acts as a bridge between wasmtime and
/// substrate host functions.
struct HostFuncHandler {
	host_func: &'static dyn Function,
}

fn call_static(
	static_func: &'static dyn Function,
	wasmtime_params: &[Val],
	wasmtime_results: &mut [Val],
) -> Result<(), wasmtime::Trap> {
	let unwind_result = state_holder::with_context(|host_ctx| {
		let mut host_ctx = host_ctx.expect(
			"host functions can be called only from wasm instance;
			wasm instance is always called initializing context;
			therefore host_ctx cannot be None;
			qed
			",
		);
		// `into_value` panics if it encounters a value that doesn't fit into the values
		// available in substrate.
		//
		// This, however, cannot happen since the signature of this function is created from
		// a `dyn Function` signature of which cannot have a non substrate value by definition.
		let mut params = wasmtime_params.iter().cloned().map(into_value);

		std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			static_func.execute(&mut host_ctx, &mut params)
		}))
	});

	let execution_result = match unwind_result {
		Ok(execution_result) => execution_result,
		Err(err) => return Err(Trap::new(stringify_panic_payload(err))),
	};

	match execution_result {
		Ok(Some(ret_val)) => {
			debug_assert!(
				wasmtime_results.len() == 1,
				"wasmtime function signature, therefore the number of results, should always \
				correspond to the number of results returned by the host function",
			);
			wasmtime_results[0] = into_wasmtime_val(ret_val);
			Ok(())
		}
		Ok(None) => {
			debug_assert!(
				wasmtime_results.len() == 0,
				"wasmtime function signature, therefore the number of results, should always \
				correspond to the number of results returned by the host function",
			);
			Ok(())
		}
		Err(msg) => Err(Trap::new(msg)),
	}
}

impl HostFuncHandler {
	fn new(host_func: &'static dyn Function) -> Self {
		Self {
			host_func,
		}
	}

	fn into_extern(self, module: &Module) -> Extern {
		let host_func = self.host_func;
		let func_ty = wasmtime_func_sig(self.host_func);
		let func = Func::new(module.store(), func_ty,
			move |_, params, result| {
				call_static(host_func, params, result)
			}
		);
		Extern::Func(func)
	}
}

/// A `Callable` handler for missing functions.
struct MissingHostFuncHandler {
	module: String,
	name: String,
}

impl MissingHostFuncHandler {
	fn new(import_ty: &ImportType) -> Self {
		Self {
			module: import_ty.module().to_string(),
			name: import_ty.name().to_string(),
		}
	}

	fn into_extern(self, wasmtime_module: &Module, func_ty: &FuncType) -> Extern {
		let Self { module, name } = self;
		let func = Func::new(wasmtime_module.store(), func_ty.clone(),
			move |_, _, _| Err(Trap::new(format!(
				"call to a missing function {}:{}",
				module, name
			)))
		);
		Extern::Func(func)
	}
}

fn wasmtime_func_sig(func: &dyn Function) -> wasmtime::FuncType {
	let params = func
		.signature()
		.args
		.iter()
		.cloned()
		.map(into_wasmtime_val_type)
		.collect::<Vec<_>>()
		.into_boxed_slice();
	let results = func
		.signature()
		.return_value
		.iter()
		.cloned()
		.map(into_wasmtime_val_type)
		.collect::<Vec<_>>()
		.into_boxed_slice();
	wasmtime::FuncType::new(params, results)
}

fn into_wasmtime_val_type(val_ty: ValueType) -> wasmtime::ValType {
	match val_ty {
		ValueType::I32 => wasmtime::ValType::I32,
		ValueType::I64 => wasmtime::ValType::I64,
		ValueType::F32 => wasmtime::ValType::F32,
		ValueType::F64 => wasmtime::ValType::F64,
	}
}

/// Converts a `Val` into a substrate runtime interface `Value`.
///
/// Panics if the given value doesn't have a corresponding variant in `Value`.
fn into_value(val: Val) -> Value {
	match val {
		Val::I32(v) => Value::I32(v),
		Val::I64(v) => Value::I64(v),
		Val::F32(f_bits) => Value::F32(f_bits),
		Val::F64(f_bits) => Value::F64(f_bits),
		_ => panic!("Given value type is unsupported by substrate"),
	}
}

fn into_wasmtime_val(value: Value) -> wasmtime::Val {
	match value {
		Value::I32(v) => Val::I32(v),
		Value::I64(v) => Val::I64(v),
		Value::F32(f_bits) => Val::F32(f_bits),
		Value::F64(f_bits) => Val::F64(f_bits),
	}
}

/// Attempt to convert a opaque panic payload to a string.
fn stringify_panic_payload(payload: Box<dyn Any + Send + 'static>) -> String {
	match payload.downcast::<&'static str>() {
		Ok(msg) => msg.to_string(),
		Err(payload) => match payload.downcast::<String>() {
			Ok(msg) => *msg,
			// At least we tried...
			Err(_) => "Box<Any>".to_string(),
		},
	}
}
