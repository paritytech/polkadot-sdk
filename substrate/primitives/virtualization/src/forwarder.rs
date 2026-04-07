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
	host_fn, ExecAction, ExecBuffer, ExecError, ExecOutcome, InstantiateError, MemoryError,
	MemoryT, VirtT,
};
use sp_wasm_interface::InstanceId;

#[cfg(not(substrate_runtime))]
use crate::ExecStatus;

/// The forwarder implementation of [`VirtT`].
pub struct Virt {
	/// The is passed to the host function to identify the instance to operate on.
	instance_id: InstanceId,
}

/// The forwarder implementation of [`MemoryT`].
pub struct Memory {
	instance_id: InstanceId,
}

impl VirtT for Virt {
	type Memory = Memory;

	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError> {
		let instance_id = InstanceId(host_fn::instantiate(program)?);
		let virt = Self { instance_id };
		Ok(virt)
	}

	fn run(&mut self, gas_left: i64, action: ExecAction<'_>) -> Result<ExecOutcome, ExecError> {
		let mut buf = ExecBuffer::default();
		let status_byte = match action {
			ExecAction::Execute(function) => {
				host_fn::execute(self.instance_id.0, function, gas_left, &mut buf)?
			},
			ExecAction::Resume(return_value) => {
				host_fn::resume(self.instance_id.0, gas_left, return_value, &mut buf)?
			},
		};
		let status = status_byte.try_into().expect("invalid status from host; qed");
		Ok(buf.into_outcome(status))
	}

	fn memory(&self) -> Self::Memory {
		Memory { instance_id: self.instance_id }
	}
}

impl Drop for Virt {
	fn drop(&mut self) {
		host_fn::destroy(self.instance_id.0).ok();
	}
}

impl MemoryT for Memory {
	fn read(&mut self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError> {
		host_fn::read_memory(self.instance_id.0, offset, dest)
	}

	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError> {
		host_fn::write_memory(self.instance_id.0, offset, src)
	}
}
