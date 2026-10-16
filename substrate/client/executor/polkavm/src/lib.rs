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
	Function, FunctionContext as WasmFunctionContext, HostFunctions, Pointer, Value, ValueType,
	WordSize,
};

/// The largest call payload a JAM runtime blob can be handed, in bytes.
///
/// A JAM blob's picoalloc allocator places its arena at the heap base without consulting `sbrk`,
/// so a payload written there is overwritten by the first allocation. JAM blobs get it in the
/// aux-data region instead, which the allocator never touches; this is that region's maximum.
const JAM_MAX_INPUT_SIZE: u32 = 64 * 1024 * 1024;

/// State made available to host functions for the duration of a runtime call.
///
/// A runtime blob's entry points take `(input_data, input_len)` on riscv64 (the ABI the `sp-api`
/// macro emits), so the call payload is written into guest memory before the call and the guest
/// reads it as a slice; the JAM host-call set the runtime may call is registered on the linker.
#[derive(Default)]
struct HostState;

// Cloning is cheap: `polkavm::InstancePre` is an `Arc` sharing the compiled module and linked
// host functions.
#[derive(Clone)]
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

		// A JAM blob (created with an aux-data region) reads its payload from there.
		let aux_size = self.0.module().memory_map().aux_data_size();
		let data_pointer = if aux_size > 0 {
			if let Err(err) = self.0.set_accessible_aux_size(raw_data_length) {
				return (
					Err(format!(
						"call into the runtime method '{name}' failed: input payload of {raw_data_length} bytes does not fit the {aux_size}-byte input region: {err}"
					)
					.into()),
					None,
				);
			}
			self.0.module().memory_map().aux_data_address()
		} else {
			// Otherwise allocate space for the payload at the start of the heap.
			if let Err(err) = self.0.sbrk(raw_data_length) {
				return (
					Err(format!(
						"call into the runtime method '{name}' failed: reset memory failed: {err}"
					)
					.into()),
					None,
				);
			}
			self.0.module().memory_map().heap_base()
		};

		if let Err(err) = self.0.write_memory(data_pointer, raw_data) {
			return (Err(format!("call into the runtime method '{name}': failed to write the input payload into guest memory: {err}").into()), None);
		}

		let mut state = HostState;

		match self.0.call_typed(&mut state, pc, (data_pointer, raw_data_length)) {
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

impl<'r, 'a> WasmFunctionContext for Context<'r, 'a> {
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
}

/// The node side of the JAM host-call set, as seen from the PolkaVM executor.
///
/// Guest pointers are plain addresses; this adapter is the
/// [`sp_jam_runtime_interface::FunctionContext`] the host-call bodies use to reach the guest's
/// memory.
struct JamContext<'r, 'a>(&'r mut polkavm::Caller<'a, HostState>);
impl sp_jam_runtime_interface::FunctionContext for JamContext<'_, '_> {
	fn read_memory_into(&mut self, ptr: *const u8, dest: &mut [u8]) -> Result<(), String> {
		self.0
			.instance
			.read_memory_into(ptr as u32, dest)
			.map_err(|error| error.to_string())
			.map(|_| ())
	}

	fn write_memory(&mut self, ptr: *mut u8, data: &[u8]) -> Result<(), String> {
		self.0
			.instance
			.write_memory(ptr as u32, data)
			.map_err(|error| error.to_string())
	}
}

/// Serve one JAM host call: every argument and the return value is one register.
fn call_jam_host_function(
	caller: &mut Caller<HostState>,
	function: &dyn sp_jam_runtime_interface::HostFunction,
) -> Result<(), String> {
	struct Regs([u64; sp_jam_runtime_interface::MAX_ARGS]);

	impl sp_jam_runtime_interface::Registers for Regs {
		fn arg(&self, index: usize) -> u64 {
			self.0[index]
		}
	}

	let mut regs = [0u64; sp_jam_runtime_interface::MAX_ARGS];
	for (index, reg) in Reg::ARG_REGS.iter().enumerate() {
		regs[index] = caller.instance.reg(*reg);
	}

	log::trace!("Calling JAM host function: '{}', args = {:?}", function.name(), regs);

	let value = function.execute(&mut JamContext(caller), &Regs(regs)).map_err(|error| {
		format!("call into the host function '{}' failed: {error}", function.name())
	})?;

	if let Some(value) = value {
		caller.instance.set_reg(Reg::A0, value);
	}

	Ok(())
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

pub fn create_runtime<H, J>(blob: &polkavm::ProgramBlob) -> Result<InstancePre, WasmError>
where
	H: HostFunctions,
	J: sp_jam_runtime_interface::HostFunctions,
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

	// JAM builds (`--cfg jam`) are identified by the `jam_validate_block` export that
	// `register_validate_block!` emits only for them.
	let is_jam = blob.exports().any(|export| export.symbol() == "jam_validate_block");

	let mut module_config = polkavm::ModuleConfig::default();
	if is_jam {
		module_config.set_aux_data_size(JAM_MAX_INPUT_SIZE);
	}
	let module = polkavm::Module::from_blob(&engine, &module_config, blob.clone())?;

	let mut linker = polkavm::Linker::<HostState, String>::new();

	for function in H::host_functions() {
		linker.define_untyped(function.name(), |mut caller: Caller<HostState>| {
			call_host_function(&mut caller, function)
		})?;
	}

	// The JAM host-call set (fixed PolkaVM indices): the runtime imports these by number.
	for function in J::host_functions() {
		linker.define_untyped(function.name(), |mut caller: Caller<HostState>| {
			call_jam_host_function(&mut caller, function)
		})?;
	}

	// `grow_heap` is not part of the JAM host-call set (the service provides it); the blob's
	// allocator imports it directly. It keeps the historical byte-delta `sbrk` contract for
	// non-JAM blobs, while a JAM blob (identified by the `jam_validate_block` export) passes an
	// absolute page count, as `jam-pvm-common`'s picoalloc does.

	linker.define_untyped("grow_heap", move |caller: Caller<HostState>| {
		if !is_jam {
			let size = caller.instance.reg(Reg::A0) as u32;
			match caller.instance.sbrk(size) {
				Ok(Some(ptr)) => caller.instance.set_reg(Reg::A0, ptr as u64),
				Ok(None) => caller.instance.set_reg(Reg::A0, 0),
				Err(e) => return Err(e.to_string()),
			}
			return Ok(());
		}

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

		// The heap is already large enough: report the current page count, which is `>= requested`
		// and therefore a success for the guest.
		if requested <= top_pages {
			caller.instance.set_reg(Reg::A0, top_pages);
			return Ok(());
		}

		// Grow by whole pages; a request that does not fit is refused by reporting the current
		// (smaller) page count.
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
	Ok(InstancePre(instance_pre))
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
