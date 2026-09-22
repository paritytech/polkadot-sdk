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

use polkavm::{CallError, Caller, Reg};
use sc_executor_common::{
	error::{Error, WasmError},
	wasm_runtime::{AllocationStats, HeapAllocStrategy, WasmInstance, WasmModule},
};
use sp_externalities::ExternalitiesExt as _;
use sp_runtime_interface::unpack_ptr_and_len;
use sp_wasm_interface::{
	Function, FunctionContext, HostFunctions, Pointer, Value, ValueType, WordSize,
};

// This executor supports only RFC-145 V2 entry points. A V2 entry point takes a single
// `input_len` argument (zero-extended into A0 on riscv64) and pulls its payload out of the
// host via the `Input::read` host function, which stashes the payload in `HostState::input_data`
// for the duration of a runtime call. There is no V1 (pointer+length) convention here.

/// State made available to host functions for the duration of a runtime call.
struct HostState {
	/// Input payload of the current call, taken by `Input::read` on V2 entry points.
	input_data: Option<Vec<u8>>,
}

#[repr(transparent)]
pub struct InstancePre(polkavm::InstancePre<HostState, String>);

#[repr(transparent)]
pub struct Instance(polkavm::Instance<HostState, String>);

impl WasmModule for InstancePre {
	fn new_instance(
		&self,
		_heap_alloc_strategy: HeapAllocStrategy,
	) -> Result<Box<dyn WasmInstance>, Error> {
		Ok(Box::new(Instance(self.0.instantiate()?)))
	}
}

impl WasmInstance for Instance {
	fn call_with_allocation_stats(
		&mut self,
		name: &str,
		raw_data: &[u8],
	) -> (Result<Vec<u8>, Error>, Option<AllocationStats>) {
		let pc = match self.0.module().exports().find(|e| e.symbol() == name) {
			Some(export) => export.program_counter(),
			None => {
				return (
					Err(format!("cannot call into the runtime: export not found: '{name}'").into()),
					None,
				)
			},
		};

		let Ok(raw_data_length) = u32::try_from(raw_data.len()) else {
			return (
				Err(format!("cannot call runtime method '{name}': input payload is too big").into()),
				None,
			);
		};

		// Make sure that the memory is cleared...
		if let Err(err) = self.0.reset_memory() {
			return (
				Err(format!(
					"call into the runtime method '{name}' failed: reset memory failed: {err}"
				)
				.into()),
				None,
			);
		}

		// Stash the input payload so the guest can pull it out via `Input::read` (RFC-145 V2).
		let mut state = HostState { input_data: Some(raw_data.to_vec()) };

		match self.0.call_typed(&mut state, pc, (raw_data_length,)) {
			Ok(()) => {},
			Err(CallError::Trap) => {
				return (
					Err(format!("call into the runtime method '{name}' failed: trap").into()),
					None,
				)
			},
			Err(CallError::Error(err)) => {
				return (
					Err(format!("call into the runtime method '{name}' failed: {err}").into()),
					None,
				)
			},
			Err(CallError::User(err)) => {
				return (
					Err(format!("call into the runtime method '{name}' failed: {err}").into()),
					None,
				)
			},
			Err(CallError::NotEnoughGas) => unreachable!("gas metering is never enabled"),
			Err(CallError::Step) => unreachable!("stepping is never enabled"),
		};

		let result = self.0.reg(Reg::A0);
		let (result_pointer, result_length) = unpack_ptr_and_len(result);
		let output = match self.0.read_memory(result_pointer, result_length) {
			Ok(output) => output,
			Err(error) => {
				return (Err(format!("call into the runtime method '{name}' failed: failed to read the return payload: {error}").into()), None)
			},
		};

		(Ok(output), None)
	}
}

struct Context<'r, 'a>(&'r mut polkavm::Caller<'a, HostState>);

impl<'r, 'a> FunctionContext for Context<'r, 'a> {
	fn read_memory_into(
		&mut self,
		address: Pointer<u8>,
		dest: &mut [u8],
	) -> sp_wasm_interface::Result<()> {
		self.0
			.instance
			.read_memory_into(u32::from(address), dest)
			.map_err(|error| error.to_string())
			.map(|_| ())
	}

	fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
		self.0
			.instance
			.write_memory(u32::from(address), data)
			.map_err(|error| error.to_string())
	}

	fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
		let pointer = match self.0.instance.sbrk(0) {
			Ok(pointer) => pointer.expect("fetching the current heap pointer never fails"),
			Err(err) => return Err(format!("sbrk failed: {err}")),
		};

		// TODO: This will leak guest memory; find a better solution.
		match self.0.instance.sbrk(size) {
			Ok(Some(_)) => (),
			Ok(None) => return Err(String::from("allocation error")),
			Err(err) => return Err(format!("sbrk failed: {err}")),
		}

		Ok(Pointer::new(pointer))
	}

	fn deallocate_memory(&mut self, _ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
		// This is only used by the allocator host function, which is unused under PolkaVM.
		unimplemented!("'deallocate_memory' is never used when running under PolkaVM");
	}

	fn register_panic_error_message(&mut self, _message: &str) {
		unimplemented!("'register_panic_error_message' is never used when running under PolkaVM");
	}

	fn take_input_data(&mut self) -> sp_wasm_interface::Result<Vec<u8>> {
		self.0
			.user_data
			.input_data
			.take()
			.ok_or_else(|| "Input data already taken".into())
	}
}

fn call_host_function(
	caller: &mut Caller<HostState>,
	function: &dyn Function,
) -> Result<(), String> {
	let mut args = [Value::I64(0); Reg::ARG_REGS.len()];
	let mut nth_reg = 0;
	for (nth_arg, kind) in function.signature().args.iter().enumerate() {
		match kind {
			ValueType::I32 => {
				args[nth_arg] = Value::I32(caller.instance.reg(Reg::ARG_REGS[nth_reg]) as i32);
				nth_reg += 1;
			},
			ValueType::F32 => {
				args[nth_arg] = Value::F32(caller.instance.reg(Reg::ARG_REGS[nth_reg]) as u32);
				nth_reg += 1;
			},
			ValueType::I64 => {
				if caller.instance.is_64_bit() {
					args[nth_arg] = Value::I64(caller.instance.reg(Reg::ARG_REGS[nth_reg]) as i64);
					nth_reg += 1;
				} else {
					let value_lo = caller.instance.reg(Reg::ARG_REGS[nth_reg]);
					nth_reg += 1;

					let value_hi = caller.instance.reg(Reg::ARG_REGS[nth_reg]);
					nth_reg += 1;

					args[nth_arg] =
						Value::I64((u64::from(value_lo) | (u64::from(value_hi) << 32)) as i64);
				}
			},
			ValueType::F64 => {
				if caller.instance.is_64_bit() {
					args[nth_arg] = Value::F64(caller.instance.reg(Reg::ARG_REGS[nth_reg]));
					nth_reg += 1;
				} else {
					let value_lo = caller.instance.reg(Reg::ARG_REGS[nth_reg]);
					nth_reg += 1;

					let value_hi = caller.instance.reg(Reg::ARG_REGS[nth_reg]);
					nth_reg += 1;

					args[nth_arg] = Value::F64(u64::from(value_lo) | (u64::from(value_hi) << 32));
				}
			},
		}
	}

	log::trace!(
		"Calling host function: '{}', args = {:?}",
		function.name(),
		&args[..function.signature().args.len()]
	);

	let value = match function
		.execute(&mut Context(caller), &mut args.into_iter().take(function.signature().args.len()))
	{
		Ok(value) => value,
		Err(error) => {
			let name = function.name();
			return Err(format!("call into the host function '{name}' failed: {error}"));
		},
	};

	if let Some(value) = value {
		match value {
			Value::I32(value) => {
				caller.instance.set_reg(Reg::A0, value as u64);
			},
			Value::F32(value) => {
				caller.instance.set_reg(Reg::A0, value as u64);
			},
			Value::I64(value) => {
				if caller.instance.is_64_bit() {
					caller.instance.set_reg(Reg::A0, value as u64);
				} else {
					caller.instance.set_reg(Reg::A0, value as u64);
					caller.instance.set_reg(Reg::A1, (value >> 32) as u64);
				}
			},
			Value::F64(value) => {
				if caller.instance.is_64_bit() {
					caller.instance.set_reg(Reg::A0, value as u64);
				} else {
					caller.instance.set_reg(Reg::A0, value as u64);
					caller.instance.set_reg(Reg::A1, (value >> 32) as u64);
				}
			},
		}
	}

	Ok(())
}

sp_externalities::decl_extension! {
	/// Externalities extension backing the JAM `fetch` host call during local block authoring.
	///
	/// The collator registers this per authoring round with the SCALE-encoded `RefineContext` of
	/// the work package being executed, so a runtime calling
	/// `parachain_service_core::refine::refine_context()` resolves it here instead of trapping on
	/// the missing `fetch` host call. The service serves the same data on the PVF path.
	pub struct JamRefineContextExt(Vec<u8>);
}

/// `jam_types::FetchKind::RefineContext`.
const FETCH_KIND_REFINE_CONTEXT: u64 = 10;

/// `jam_types::SimpleResultCode::Nothing`, the JAM `NONE` host-call result.
const SIMPLE_RESULT_NOTHING: u64 = u64::MAX;

fn registered_refine_context() -> Option<Vec<u8>> {
	sp_externalities::with_externalities(|mut ext| {
		ext.extension::<JamRefineContextExt>().map(|ext| ext.0.clone())
	})
	.flatten()
}

/// Clamp a fetched item to the caller's window, mirroring the service's `get_slice`.
///
/// The full length is returned even for an empty window: the guest's `fetch()` sizes its buffer
/// with an empty first pass and fetches again with the real one.
fn fetch_window(data: &[u8], offset: u64, len: u64) -> (u64, &[u8]) {
	let data_len = data.len() as u64;
	let offset = offset.min(data_len);
	let len = len.min(data_len - offset);
	(data_len, &data[offset as usize..(offset + len) as usize])
}

/// Serve one JAM `fetch` host call, returning the `SimpleResult` for `A0`.
///
/// Only `FetchKind::RefineContext` is served locally; every other kind is the PVF's to answer.
/// `write` is skipped for an empty window, so the guest's sizing pass touches no memory.
fn serve_fetch(
	kind: u64,
	offset: u64,
	len: u64,
	mut write: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<u64, String> {
	if kind != FETCH_KIND_REFINE_CONTEXT {
		return Ok(SIMPLE_RESULT_NOTHING);
	}
	let Some(data) = registered_refine_context() else {
		return Ok(SIMPLE_RESULT_NOTHING);
	};
	let (data_len, window) = fetch_window(&data, offset, len);
	if !window.is_empty() {
		write(window)?;
	}
	Ok(data_len)
}

pub fn create_runtime<H>(blob: &polkavm::ProgramBlob) -> Result<Box<dyn WasmModule>, WasmError>
where
	H: HostFunctions,
{
	static ENGINE: std::sync::OnceLock<Result<polkavm::Engine, polkavm::Error>> =
		std::sync::OnceLock::new();

	let engine = ENGINE.get_or_init(|| {
		let config = polkavm::Config::from_env()?;
		polkavm::Engine::new(&config)
	});

	let engine = match engine {
		Ok(ref engine) => engine,
		Err(ref error) => {
			return Err(WasmError::Other(error.to_string()));
		},
	};

	let module =
		polkavm::Module::from_blob(&engine, &polkavm::ModuleConfig::default(), blob.clone())?;

	let mut linker = polkavm::Linker::<HostState, String>::new();

	for function in H::host_functions() {
		linker.define_untyped(function.name(), |mut caller: Caller<HostState>| {
			call_host_function(&mut caller, function)
		})?;
	}

	// Temporary shim: `sbrk` was removed from the `jam_v1` instruction set (GP 0.8.0)
	// and replaced with a `grow_heap` host call. The guest-side allocator in
	// `sp-io` imports this symbol.
	linker.define_untyped("grow_heap", |caller: Caller<HostState>| {
		let size = caller.instance.reg(Reg::A0) as u32;
		match caller.instance.sbrk(size) {
			Ok(Some(ptr)) => caller.instance.set_reg(Reg::A0, ptr as u64),
			Ok(None) => caller.instance.set_reg(Reg::A0, 0),
			Err(e) => return Err(e.to_string()),
		}
		Ok(())
	})?;

	// JAM `fetch` host call (Gray Paper index 2). The runtime imports it directly through
	// `jam_pvm_common::imports::fetch`, so like `grow_heap` it needs a raw linker definition; the
	// collator registers the per-block refine context as a [`JamRefineContextExt`].
	linker.define_untyped("fetch", |caller: Caller<HostState>| {
		let buffer = caller.instance.reg(Reg::A0) as u32;
		let offset = caller.instance.reg(Reg::A1);
		let len = caller.instance.reg(Reg::A2);
		let kind = caller.instance.reg(Reg::A3);
		let result = serve_fetch(kind, offset, len, |window| {
			caller.instance.write_memory(buffer, window).map_err(|error| error.to_string())
		})?;
		caller.instance.set_reg(Reg::A0, result);
		Ok(())
	})?;

	let instance_pre = linker.instantiate_pre(&module)?;
	Ok(Box::new(InstancePre(instance_pre)))
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_state_machine::BasicExternalities;

	fn sink(written: &mut Vec<u8>) -> impl FnMut(&[u8]) -> Result<(), String> + '_ {
		move |window| {
			written.extend_from_slice(window);
			Ok(())
		}
	}

	#[test]
	fn fetch_refine_context_serves_the_registered_context() {
		let context = (0u8..169).collect::<Vec<u8>>();
		let mut ext = BasicExternalities::default();
		ext.register_extension(JamRefineContextExt(context.clone()));

		ext.execute_with(|| {
			// The guest's `fetch()` sizes with an empty window first, then fetches again.
			let mut written = Vec::new();
			let len = serve_fetch(FETCH_KIND_REFINE_CONTEXT, 0, 0, sink(&mut written)).unwrap();
			assert_eq!(len, context.len() as u64);
			assert!(written.is_empty(), "the sizing pass must not write");

			let mut written = Vec::new();
			let len = serve_fetch(FETCH_KIND_REFINE_CONTEXT, 0, 4, sink(&mut written)).unwrap();
			assert_eq!(len, context.len() as u64);
			assert_eq!(written, context[..4]);

			let mut written = Vec::new();
			let len = serve_fetch(FETCH_KIND_REFINE_CONTEXT, 2, 3, sink(&mut written)).unwrap();
			assert_eq!(len, context.len() as u64);
			assert_eq!(written, context[2..5]);
		});
	}

	#[test]
	fn fetch_refine_context_is_nothing_when_not_registered() {
		BasicExternalities::default().execute_with(|| {
			let mut written = Vec::new();
			let result = serve_fetch(FETCH_KIND_REFINE_CONTEXT, 0, 0, sink(&mut written)).unwrap();
			assert_eq!(result, SIMPLE_RESULT_NOTHING);
			assert!(written.is_empty());
		});
	}

	#[test]
	fn fetch_other_kinds_are_not_served_locally() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(JamRefineContextExt(vec![1, 2, 3]));

		ext.execute_with(|| {
			let mut written = Vec::new();
			// Kind 0 is `ProtocolParameters`, which only the PVF serves.
			let result = serve_fetch(0, 0, 0, sink(&mut written)).unwrap();
			assert_eq!(result, SIMPLE_RESULT_NOTHING);
			assert!(written.is_empty());
		});
	}
}
