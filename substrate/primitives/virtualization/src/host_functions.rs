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

use crate::{DestroyError, ExecError, InstantiateError, MemoryError};
use sp_runtime_interface::runtime_interface;

/// Host functions used to spawn and call into PolkaVM instances.
///
/// Use [`crate::Virt`] instead of these raw host functions. This will also make sure that
/// everything works when running the code in native (test code) as this is a `wasm_only` interface.
///
/// # Warning
///
/// This is an unstable API. Its behaviour is subject to change until there is a spec. Don't use
/// this API in your runtime except for test purposes.
#[runtime_interface(wasm_only)]
pub trait Virtualization {
	/// See `sp_virtualization::Virt::instantiate`.
	///
	/// Returns the `instance_id` which needs to be passed to reference this instance
	/// when using the other functions of this trait.
	fn instantiate(&mut self, program: &[u8]) -> Result<u64, InstantiateError> {
		self.virtualization()
			.instantiate(program)
			.expect("instantiation failed")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// See `sp_virtualization::Virt::instantiate`.
	///
	/// # Arguments
	///
	/// * `instance_id`: The id returned from [`Self::instantiate`].
	/// * `function`: Same as in `sp_virtualization::Virt::execute`.
	/// * `syscall_handler`: Pointer to a [`VirtSyscallHandler<T>`].
	/// * `state_ptr`: Pointer to a [`VirtSharedState<T>`].
	/// * `destroy`: True if the instance should be destroyed after execution. Useful if no further
	///   calls or memory reads are necessary.
	fn execute(
		&mut self,
		instance_id: u64,
		function: &str,
		syscall_handler: u32,
		state_ptr: u32,
		destroy: bool,
	) -> Result<(), ExecError> {
		self.virtualization()
			.execute(instance_id, function, syscall_handler, state_ptr, destroy)
			.expect("execution failed")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// Destroy this instance.
	///
	/// Any attempt accessing an instance after destruction will yield the `InvalidInstance` error.
	fn destroy(&mut self, instance_id: u64) -> Result<(), DestroyError> {
		self.virtualization()
			.destroy(instance_id)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// See `sp_virtualization::Memory::read`.
	fn read_memory(
		&mut self,
		instance_id: u64,
		offset: u32,
		dest: &mut [u8],
	) -> Result<(), MemoryError> {
		self.virtualization()
			.read_memory(instance_id, offset, dest)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// See `sp_virtualization::Memory::write`.
	fn write_memory(
		&mut self,
		instance_id: u64,
		offset: u32,
		src: &[u8],
	) -> Result<(), MemoryError> {
		self.virtualization()
			.write_memory(instance_id, offset, src)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}
}
