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
	host_functions::{virtualization as host_fn, ExecBuffer, ExecStatus},
	ExecError, ExecResult, InstanceId, InstantiateError, MemoryError, ModuleError, ModuleId,
};

/// A compiled module handle.
pub struct Module(ModuleId);

impl Module {
	/// Compile a module from raw bytes.
	///
	/// Always compiles; if `identifier` is `Some` the result is cached under it so a later
	/// [`Module::lookup`] with the same `identifier` reuses it. Callers that want to avoid
	/// recompiling an already-cached module should [`Module::lookup`] first and only call this
	/// on a miss.
	pub fn from_bytes(bytes: &[u8], identifier: Option<&[u8]>) -> Result<Self, ModuleError> {
		Ok(Self(host_fn::compile(bytes, identifier)?))
	}

	/// Look up a previously compiled module by `identifier`.
	///
	/// Returns `Err(ModuleError::NotCached)` if no module is cached under `identifier`.
	/// This is a pure cache lookup, without storage access or compilation, so the call is
	/// cheap. On a miss, compile the bytes with [`Module::from_bytes`].
	pub fn lookup(identifier: &[u8]) -> Result<Self, ModuleError> {
		Ok(Self(host_fn::lookup(identifier)?))
	}

	pub fn instantiate(&self) -> Result<Instance, InstantiateError> {
		Ok(Instance(host_fn::instantiate(self.0)?))
	}
}

/// An idle virtualization instance.
pub struct Instance(InstanceId);

impl Drop for Instance {
	fn drop(&mut self) {
		host_fn::destroy(self.0).ok();
	}
}

impl core::fmt::Debug for Instance {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("Instance").field(&self.0).finish()
	}
}

impl Instance {
	pub fn prepare(self, function: &[u8]) -> Result<Execution, (Self, ExecError)> {
		match host_fn::prepare(self.0, function) {
			Ok(()) => {
				let instance_id = self.0;
				core::mem::forget(self);
				Ok(Execution(instance_id))
			},
			Err(err) => Err((self, err)),
		}
	}
}

/// A prepared or suspended virtualization execution.
pub struct Execution(InstanceId);

impl Drop for Execution {
	fn drop(&mut self) {
		host_fn::destroy(self.0).ok();
	}
}

impl Execution {
	pub fn run(self, gas_left: i64, a0: u64) -> ExecResult<Instance, Self> {
		let mut buf = ExecBuffer::default();
		let status_byte = match host_fn::run(self.0, gas_left, a0, &mut buf) {
			Ok(s) => s,
			Err(err) => {
				let instance_id = self.0;
				core::mem::forget(self);
				return ExecResult::Error { instance: Instance(instance_id), error: err };
			},
		};
		let status: ExecStatus = status_byte.try_into().expect("invalid status from host; qed");
		match status {
			ExecStatus::Finished => {
				let instance_id = self.0;
				core::mem::forget(self);
				ExecResult::Finished { instance: Instance(instance_id), gas_left: buf.gas_left }
			},
			ExecStatus::Syscall => ExecResult::Syscall {
				execution: self,
				gas_left: buf.gas_left,
				syscall_symbol: buf.syscall_symbol,
				a0: buf.a0,
				a1: buf.a1,
				a2: buf.a2,
				a3: buf.a3,
				a4: buf.a4,
				a5: buf.a5,
			},
		}
	}

	pub fn read_memory(&mut self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		host_fn::read_memory(self.0, offset, dest)
	}

	pub fn write_memory(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		host_fn::write_memory(self.0, offset, src)
	}
}
