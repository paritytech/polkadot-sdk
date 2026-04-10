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

//! Manages virtualization instances. It is used by the host function **implementation**.

use crate::{DestroyError, ExecError, Memory, MemoryError, MemoryT, Virt, VirtT};
use sp_wasm_interface::{ExecAction, ExecOutcome, InstanceId, Virtualization};
use std::collections::HashMap;

/// A virtualization instance held by [`VirtManager`].
struct VirtInstance {
	virt: Virt,
	memory: Memory,
}

/// Manages virtualization instances and their lifecycle.
///
/// Instance IDs are assigned deterministically from an incrementing counter,
/// ensuring no non-determinism across different executions.
pub struct VirtManager {
	instances: HashMap<InstanceId, VirtInstance>,
	counter: u32,
}

impl Default for VirtManager {
	fn default() -> Self {
		Self { instances: HashMap::new(), counter: 0 }
	}
}

impl Virtualization for VirtManager {
	fn instantiate(&mut self, program: &[u8]) -> sp_wasm_interface::Result<Result<InstanceId, u8>> {
		let virt = match Virt::instantiate(program) {
			Ok(virt) => virt,
			Err(err) => return Ok(Err(err.into())),
		};

		let instance_id = InstanceId({
			let old = self.counter;
			self.counter = old + 1;
			old
		});

		self.instances.insert(instance_id, VirtInstance { memory: virt.memory(), virt });

		Ok(Ok(instance_id))
	}

	fn run(
		&mut self,
		instance_id: InstanceId,
		gas_left: i64,
		action: ExecAction<'_>,
	) -> sp_wasm_interface::Result<Result<ExecOutcome, u8>> {
		let instance = match self.instances.get_mut(&instance_id) {
			Some(instance) => instance,
			None => return Ok(Err(ExecError::InvalidInstance.into())),
		};

		let result = instance.virt.run(gas_left, action);
		Ok(result.map_err(|err| err.into()))
	}

	fn destroy(&mut self, instance_id: InstanceId) -> sp_wasm_interface::Result<Result<(), u8>> {
		if self.instances.remove(&instance_id).is_some() {
			Ok(Ok(()))
		} else {
			Ok(Err(DestroyError::InvalidInstance.into()))
		}
	}

	fn read_memory(
		&mut self,
		instance_id: InstanceId,
		offset: u32,
		dest: &mut [u8],
	) -> sp_wasm_interface::Result<Result<(), u8>> {
		let Some(instance) = self.instances.get_mut(&instance_id) else {
			return Ok(Err(MemoryError::InvalidInstance.into()));
		};
		if let Err(err) = instance.memory.read(offset, dest) {
			return Ok(Err(err.into()));
		}
		Ok(Ok(()))
	}

	fn write_memory(
		&mut self,
		instance_id: InstanceId,
		offset: u32,
		src: &[u8],
	) -> sp_wasm_interface::Result<Result<(), u8>> {
		let Some(instance) = self.instances.get_mut(&instance_id) else {
			return Ok(Err(MemoryError::InvalidInstance.into()));
		};
		if let Err(err) = instance.memory.write(offset, src) {
			return Ok(Err(err.into()));
		}
		Ok(Ok(()))
	}
}
