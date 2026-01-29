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

//! Defines data and logic needed for interaction with an WebAssembly instance of a substrate
//! runtime module.

use std::sync::Arc;

use crate::runtime::{InstanceCounter, ReleaseInstanceHandle, Store, StoreData};
use sc_executor_common::error::{Backtrace, Error, MessageWithBacktrace, Result, WasmError};
use sp_wasm_interface::{Pointer, WordSize};
use wasmtime::{AsContext, AsContextMut, Engine, Instance, InstancePre, Memory};

/// Wasm blob entry point.
pub enum EntryPoint {
	V1(wasmtime::TypedFunc<(u32, u32), u64>),
	V2(wasmtime::TypedFunc<(u32,), u64>),
}

impl EntryPoint {
	/// Call this entry point.
	pub(crate) fn call(&self, instance: &mut InstanceWrapper) -> Result<u64> {
		let result =
			match self {
				Self::V1(func) => {
					// SAFETY: Entry point signature has been checked statically and represents a
					// V1 entry point. The V1 code is known to use the host-side allocator.
					let (data_ptr, data_len) = unsafe { instance.inject_input_data()? };
					let data_ptr = u32::from(data_ptr);
					let data_len = u32::from(data_len);
					func.call(instance.store_mut(), (data_ptr, data_len))
				},
				Self::V2(func) => {
					let host_state =
						instance.store().data().host_state.as_ref().expect(
							"host state cannot be empty while a function is being called; qed",
						);
					let data_len = host_state
						.input_data
						.as_ref()
						.expect("input data cannot be empty while a function is being called; qed")
						.len() as u32;
					func.call(instance.store_mut(), (data_len,))
				},
			};

		result.map_err(|trap| {
			let host_state = instance
				.store_mut()
				.data_mut()
				.host_state
				.as_mut()
				.expect("host state cannot be empty while a function is being called; qed");

			let backtrace = trap.downcast_ref::<wasmtime::WasmBacktrace>().map(|backtrace| {
				// The logic to print out a backtrace is somewhat complicated,
				// so let's get wasmtime to print it out for us.
				Backtrace { backtrace_string: backtrace.to_string() }
			});

			if let Some(message) = host_state.take_panic_message() {
				Error::AbortedDueToPanic(MessageWithBacktrace { message, backtrace })
			} else {
				let message = trap.root_cause().to_string();
				Error::AbortedDueToTrap(MessageWithBacktrace { message, backtrace })
			}
		})
	}

	pub fn direct(
		func: wasmtime::Func,
		ctx: impl AsContext,
	) -> std::result::Result<Self, &'static str> {
		let ty = func.ty(ctx.as_context());
		if ty.params().len() == 1 {
			let entrypoint = func
				.typed::<(u32,), u64>(ctx)
				.map_err(|_| "Invalid signature for direct V2 entry point")?;
			Ok(Self::V2(entrypoint))
		} else {
			let entrypoint = func
				.typed::<(u32, u32), u64>(ctx)
				.map_err(|_| "Invalid signature for direct V1 entry point")?;
			Ok(Self::V1(entrypoint))
		}
	}
}

/// Wrapper around [`Memory`] that implements [`sc_allocator::Memory`].
pub(crate) struct MemoryWrapper<'a, C>(pub &'a wasmtime::Memory, pub &'a mut C);

impl<C: AsContextMut> sc_allocator::Memory for MemoryWrapper<'_, C> {
	fn with_access_mut<R>(&mut self, run: impl FnOnce(&mut [u8]) -> R) -> R {
		run(self.0.data_mut(&mut self.1))
	}

	fn with_access<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
		run(self.0.data(&self.1))
	}

	fn grow(&mut self, additional: u32) -> std::result::Result<(), ()> {
		self.0
			.grow(&mut self.1, additional as u64)
			.map_err(|e| {
				log::error!(
					target: "wasm-executor",
					"Failed to grow memory by {} pages: {}",
					additional,
					e,
				)
			})
			.map(drop)
	}

	fn pages(&self) -> u32 {
		self.0.size(&self.1) as u32
	}

	fn max_pages(&self) -> Option<u32> {
		self.0.ty(&self.1).maximum().map(|p| p as _)
	}
}

/// Wrap the given WebAssembly Instance of a wasm module with Substrate-runtime.
///
/// This struct is a handy wrapper around a wasmtime `Instance` that provides substrate specific
/// routines.
pub struct InstanceWrapper {
	instance: Instance,
	store: Store,
	// NOTE: We want to decrement the instance counter *after* the store has been dropped
	// to avoid a potential race condition, so this field must always be kept
	// as the last field in the struct!
	_release_instance_handle: ReleaseInstanceHandle,
}

impl InstanceWrapper {
	pub(crate) fn new(
		engine: &Engine,
		instance_pre: &InstancePre<StoreData>,
		instance_counter: Arc<InstanceCounter>,
	) -> Result<Self> {
		let _release_instance_handle = instance_counter.acquire_instance();
		let mut store = Store::new(engine, Default::default());
		let instance = instance_pre.instantiate(&mut store).map_err(|error| {
			WasmError::Other(format!(
				"failed to instantiate a new WASM module instance: {:#}",
				error,
			))
		})?;

		let memory = get_linear_memory(&instance, &mut store)?;

		store.data_mut().memory = Some(memory);

		Ok(InstanceWrapper { instance, store, _release_instance_handle })
	}

	/// Resolves a substrate entrypoint by the given name.
	///
	/// An entrypoint must have a signature `(i32, i32) -> i64`, otherwise this function will return
	/// an error.
	pub fn resolve_entrypoint(&mut self, method: &str) -> Result<EntryPoint> {
		// Resolve the requested method and verify that it has a proper signature.
		let export = self
			.instance
			.get_export(&mut self.store, method)
			.ok_or_else(|| Error::from(format!("Exported method {} is not found", method)))?;
		let func = export
			.into_func()
			.ok_or_else(|| Error::from(format!("Export {} is not a function", method)))?;
		EntryPoint::direct(func, &self.store).map_err(|_| {
			Error::from(format!("Exported function '{}' has invalid signature.", method))
		})
	}

	/// Reads `__heap_base: i32` global variable and returns it.
	///
	/// If it doesn't exist, not a global or of not i32 type returns an error.
	pub fn extract_heap_base(&mut self) -> Result<u32> {
		let heap_base_export = self
			.instance
			.get_export(&mut self.store, "__heap_base")
			.ok_or_else(|| Error::from("__heap_base is not found"))?;

		let heap_base_global = heap_base_export
			.into_global()
			.ok_or_else(|| Error::from("__heap_base is not a global"))?;

		let heap_base = heap_base_global
			.get(&mut self.store)
			.i32()
			.ok_or_else(|| Error::from("__heap_base is not a i32"))?;

		Ok(heap_base as u32)
	}

	/// Injects the input data into the guest's memory.
	///
	/// Should only be used for code using the host-side allocator. Otherwise will corrupt
	/// guest's memory
	pub(crate) unsafe fn inject_input_data(&mut self) -> Result<(Pointer<u8>, WordSize)> {
		let store = self.store_mut();
		let mut allocator = store
			.data_mut()
			.host_state
			.as_mut()
			.expect("host state cannot be empty while a function is being called; qed")
			.allocator
			.take()
			.expect("allocator cannot be empty while a function is being called; qed");

		let result = {
			let mut ctx = store.as_context_mut();
			let host_data = ctx.data_mut();
			let memory = host_data.memory();
			let data = host_data
				.host_state
				.as_mut()
				.expect("host state cannot be empty while a function is being called; qed")
				.input_data
				.take()
				.expect("input data cannot be empty while a function is being called; qed");

			let data_len = data.len() as WordSize;
			match allocator.allocate(&mut MemoryWrapper(&memory, &mut ctx), data_len) {
				Ok(data_ptr) => crate::util::write_memory_from(&mut ctx, data_ptr, &data[..])
					.map(|_| (data_ptr, data_len))
					.map_err(Into::into),
				Err(e) => Err(e.into()),
			}
		};

		store
			.data_mut()
			.host_state
			.as_mut()
			.expect("host state cannot be empty while a function is being called; qed")
			.allocator = Some(allocator);

		result
	}
}

/// Extract linear memory instance from the given instance.
fn get_linear_memory(instance: &Instance, ctx: impl AsContextMut) -> Result<Memory> {
	let memory_export = instance
		.get_export(ctx, "memory")
		.ok_or_else(|| Error::from("memory is not exported under `memory` name"))?;

	let memory = memory_export
		.into_memory()
		.ok_or_else(|| Error::from("the `memory` export should have memory type"))?;

	Ok(memory)
}

/// Functions related to memory.
impl InstanceWrapper {
	pub(crate) fn store(&self) -> &Store {
		&self.store
	}

	pub(crate) fn store_mut(&mut self) -> &mut Store {
		&mut self.store
	}
}
