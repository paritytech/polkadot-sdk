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

//! JAM runtime blob support: the aux-data input region, the JAM host-call set and the host calls
//! whose contract differs between JAM and non-JAM blobs.

use crate::HostState;
use polkavm::{Caller, Reg};
use sc_executor_common::error::Error;
use sp_jam_runtime_interface::HostFunction;

/// The largest call payload a JAM runtime blob can be handed, in bytes.
///
/// A JAM blob's picoalloc allocator places its arena at the heap base without consulting `sbrk`,
/// so a payload written there is overwritten by the first allocation. JAM blobs get it in the
/// aux-data region instead, which the allocator never touches; this is that region's maximum.
const JAM_MAX_INPUT_SIZE: u32 = 64 * 1024 * 1024;

/// The JAM host-call set `J`, as [`crate::create_runtime`] takes it.
pub fn host_functions<J: sp_jam_runtime_interface::HostFunctions>() -> Vec<&'static dyn HostFunction>
{
	J::host_functions()
}

/// Whether `blob` is a JAM build (`--cfg jam`).
///
/// JAM builds are identified by the `jam_validate_block` export that `register_validate_block!`
/// emits only for them.
pub(crate) fn is_jam_blob(blob: &polkavm::ProgramBlob) -> bool {
	blob.exports().any(|export| export.symbol() == "jam_validate_block")
}

/// Write-target for the call payload of a blob created with an aux-data region (a JAM blob).
///
/// Makes `raw_data_length` bytes of the region accessible and returns its address. Returns `None`
/// for a blob without an aux-data region, which takes its payload at the heap base instead.
pub(crate) fn input_region(
	instance: &mut polkavm::Instance<HostState, String>,
	name: &str,
	raw_data_length: u32,
) -> Option<Result<u32, Error>> {
	let aux_size = instance.module().memory_map().aux_data_size();
	if aux_size == 0 {
		return None;
	}
	if let Err(err) = instance.set_accessible_aux_size(raw_data_length) {
		return Some(Err(format!(
			"call into the runtime method '{name}' failed: input payload of {raw_data_length} bytes does not fit the {aux_size}-byte input region: {err}"
		)
		.into()));
	}
	Some(Ok(instance.module().memory_map().aux_data_address()))
}

/// Configure `module_config` and `linker` for `blob`.
///
/// A JAM blob gets the aux-data input region. The JAM host-call set is linked for every blob,
/// together with `grow_heap` and `fetch`, which the blob imports directly.
pub(crate) fn configure(
	blob: &polkavm::ProgramBlob,
	module_config: &mut polkavm::ModuleConfig,
	linker: &mut polkavm::Linker<HostState, String>,
	host_functions: Vec<&'static dyn HostFunction>,
) -> Result<(), polkavm::Error> {
	let is_jam = is_jam_blob(blob);

	if is_jam {
		module_config.set_aux_data_size(JAM_MAX_INPUT_SIZE);
	}

	// The JAM host-call set (fixed PolkaVM indices): the runtime imports these by number.
	for function in host_functions {
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
	// `jam_pvm_common::imports::fetch`, so like `grow_heap` it needs a raw linker definition. Only
	// the PVF serves fetch data; outside it every kind answers `NONE`, so a runtime that calls
	// `fetch` while a node executes it sees no data instead of trapping on an unresolved import.
	linker.define_untyped("fetch", |caller: Caller<HostState>| {
		caller.instance.set_reg(Reg::A0, SIMPLE_RESULT_NOTHING);
		Ok(())
	})?;

	Ok(())
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
	function: &dyn HostFunction,
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

/// `jam_types::SimpleResultCode::Nothing`, the JAM `NONE` host-call result.
const SIMPLE_RESULT_NOTHING: u64 = u64::MAX;
