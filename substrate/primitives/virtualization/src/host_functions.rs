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

use crate::{DestroyError, ExecBuffer, ExecError, InstantiateError, MemoryError, EXEC_BUFFER_SIZE};
use sp_runtime_interface::{
	pass_by::{
		ConvertAndReturnAs, PassFatPointerAndRead, PassFatPointerAndWrite, PassPointerAndWrite,
	},
	runtime_interface,
};
use strum::EnumCount;

#[cfg(not(substrate_runtime))]
use crate::ExecStatus;

#[derive(EnumCount)]
#[repr(i8)]
pub enum RIInstantiateError {
	InvalidImage = -1,
}

impl From<RIInstantiateError> for i64 {
	fn from(error: RIInstantiateError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIInstantiateError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		match value {
			-1 => Ok(RIInstantiateError::InvalidImage),
			_ => Err(()),
		}
	}
}

impl From<InstantiateError> for RIInstantiateError {
	fn from(error: InstantiateError) -> Self {
		match error {
			InstantiateError::InvalidImage => RIInstantiateError::InvalidImage,
		}
	}
}

impl From<RIInstantiateError> for InstantiateError {
	fn from(error: RIInstantiateError) -> Self {
		match error {
			RIInstantiateError::InvalidImage => InstantiateError::InvalidImage,
		}
	}
}

#[derive(EnumCount)]
#[repr(i8)]
pub enum RIExecError {
	InvalidInstance = -1,
	InvalidImage = -2,
	OutOfGas = -3,
	Trap = -4,
}

impl From<RIExecError> for i64 {
	fn from(error: RIExecError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIExecError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		match value {
			-1 => Ok(RIExecError::InvalidInstance),
			-2 => Ok(RIExecError::InvalidImage),
			-3 => Ok(RIExecError::OutOfGas),
			-4 => Ok(RIExecError::Trap),
			_ => Err(()),
		}
	}
}

impl From<RIExecError> for ExecError {
	fn from(error: RIExecError) -> Self {
		match error {
			RIExecError::InvalidInstance => ExecError::InvalidInstance,
			RIExecError::InvalidImage => ExecError::InvalidImage,
			RIExecError::OutOfGas => ExecError::OutOfGas,
			RIExecError::Trap => ExecError::Trap,
		}
	}
}

impl From<ExecError> for RIExecError {
	fn from(error: ExecError) -> Self {
		match error {
			ExecError::InvalidInstance => RIExecError::InvalidInstance,
			ExecError::InvalidImage => RIExecError::InvalidImage,
			ExecError::OutOfGas => RIExecError::OutOfGas,
			ExecError::Trap => RIExecError::Trap,
		}
	}
}

#[derive(EnumCount)]
#[repr(i8)]
pub enum RIDestroyError {
	InvalidInstance = -1,
}

impl From<RIDestroyError> for i64 {
	fn from(error: RIDestroyError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIDestroyError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		match value {
			-1 => Ok(RIDestroyError::InvalidInstance),
			_ => Err(()),
		}
	}
}

impl From<RIDestroyError> for DestroyError {
	fn from(error: RIDestroyError) -> Self {
		match error {
			RIDestroyError::InvalidInstance => DestroyError::InvalidInstance,
		}
	}
}

impl From<DestroyError> for RIDestroyError {
	fn from(error: DestroyError) -> Self {
		match error {
			DestroyError::InvalidInstance => RIDestroyError::InvalidInstance,
		}
	}
}

#[derive(EnumCount)]
#[repr(i8)]
pub enum RIMemoryError {
	InvalidInstance = -1,
	OutOfBounds = -2,
}

impl From<RIMemoryError> for i64 {
	fn from(error: RIMemoryError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIMemoryError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		match value {
			-1 => Ok(RIMemoryError::InvalidInstance),
			-2 => Ok(RIMemoryError::OutOfBounds),
			_ => Err(()),
		}
	}
}

impl From<RIMemoryError> for MemoryError {
	fn from(error: RIMemoryError) -> Self {
		match error {
			RIMemoryError::InvalidInstance => MemoryError::InvalidInstance,
			RIMemoryError::OutOfBounds => MemoryError::OutOfBounds,
		}
	}
}

impl From<MemoryError> for RIMemoryError {
	fn from(error: MemoryError) -> Self {
		match error {
			MemoryError::InvalidInstance => RIMemoryError::InvalidInstance,
			MemoryError::OutOfBounds => RIMemoryError::OutOfBounds,
		}
	}
}

// The following code is an excerpt from RFC-145 implementation (still to be adopted)
// ---vvv--- 8< CUT HERE 8< ---vvv---

/// Used to return less-than-64-bit value passed as `i64` through the FFI boundary.
/// Negative values are used to represent error variants.
pub enum RIIntResult<R, E> {
	/// Successful result
	Ok(R),
	/// Error result
	Err(E),
}

impl<R, E, OR, OE> From<Result<OR, OE>> for RIIntResult<R, E>
where
	R: From<OR>,
	E: From<OE>,
{
	fn from(result: Result<OR, OE>) -> Self {
		match result {
			Ok(value) => Self::Ok(value.into()),
			Err(error) => Self::Err(error.into()),
		}
	}
}

impl<R, E, OR, OE> From<RIIntResult<R, E>> for Result<OR, OE>
where
	OR: From<R>,
	OE: From<E>,
{
	fn from(result: RIIntResult<R, E>) -> Self {
		match result {
			RIIntResult::Ok(value) => Ok(value.into()),
			RIIntResult::Err(error) => Err(error.into()),
		}
	}
}

trait IntoI64: Into<i64> {
	const MAX: i64;
}

impl IntoI64 for u8 {
	const MAX: i64 = u8::MAX as i64;
}
impl IntoI64 for u32 {
	const MAX: i64 = u32::MAX as i64;
}

impl<R: Into<i64> + IntoI64, E: Into<i64> + strum::EnumCount> From<RIIntResult<R, E>> for i64 {
	fn from(result: RIIntResult<R, E>) -> Self {
		match result {
			RIIntResult::Ok(value) => value.into(),
			RIIntResult::Err(e) => {
				let error_code: i64 = e.into();
				assert!(
					error_code < 0 && error_code >= -(E::COUNT as i64),
					"Error variant index out of bounds"
				);
				error_code
			},
		}
	}
}

impl<R: TryFrom<i64> + IntoI64, E: TryFrom<i64> + strum::EnumCount> TryFrom<i64>
	for RIIntResult<R, E>
{
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value >= 0 && value <= R::MAX.into() {
			Ok(RIIntResult::Ok(value.try_into().map_err(|_| ())?))
		} else if value < 0 && value >= -(E::COUNT as i64) {
			Ok(RIIntResult::Err(value.try_into().map_err(|_| ())?))
		} else {
			Err(())
		}
	}
}

pub struct VoidResult;

impl IntoI64 for VoidResult {
	const MAX: i64 = 0;
}

impl From<VoidResult> for u32 {
	fn from(_: VoidResult) -> Self {
		0
	}
}

impl From<u32> for VoidResult {
	fn from(_: u32) -> Self {
		VoidResult
	}
}

impl From<()> for VoidResult {
	fn from(_: ()) -> Self {
		VoidResult
	}
}

impl From<VoidResult> for () {
	fn from(_: VoidResult) -> Self {
		()
	}
}

impl From<VoidResult> for i64 {
	fn from(_: VoidResult) -> Self {
		0
	}
}

impl TryFrom<i64> for VoidResult {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value == 0 {
			Ok(VoidResult)
		} else {
			Err(())
		}
	}
}

// ---^^^--- 8< CUT HERE 8< ---^^^---

/// Host functions used to spawn and call into PolkaVM instances.
///
/// Use [`crate::Virt`] instead of these raw host functions. This will also make sure that
/// everything works when running the code in native (test code) as this is a `wasm_only` interface.
///
/// # ⚠️ Unstable — Do Not Use in Production ⚠️
///
/// **This interface is unstable and subject to breaking changes without notice.**
///
/// These host functions are **not available on Polkadot** (or any other production
/// relay/parachain) until the API has been stabilized. If you use them in a production
/// runtime, your runtime **will break** when the API changes.
///
/// Only use for local testing, development, and experimentation on test networks.
/// There is no stability guarantee and no deprecation period.
#[runtime_interface(wasm_only)]
pub trait Virtualization {
	/// See `sp_virtualization::Virt::instantiate`.
	///
	/// Returns the `instance_id` which needs to be passed to reference this instance
	/// when using the other functions of this trait.
	fn instantiate(
		&mut self,
		program: PassFatPointerAndRead<&[u8]>,
	) -> ConvertAndReturnAs<Result<u32, InstantiateError>, RIIntResult<u32, RIInstantiateError>, i64>
	{
		use std::sync::Once;
		static WARN_ONCE: Once = Once::new();
		WARN_ONCE.call_once(|| {
			log::warn!(
				target: crate::LOG_TARGET,
				"Virtualization host functions are UNSTABLE and subject to breaking changes. \
				They are NOT available on Polkadot and using them in production will cause breakage. \
				Only use for testing and experimentation.",
			);
		});
		self.virtualization()
			.instantiate(program)
			.expect("instantiation failed")
			.map(|id| id.0)
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// Start execution of a function on the given instance.
	///
	/// Returns [`ExecStatus::Finished`] or [`ExecStatus::Syscall`] as `u8`.
	/// When a syscall occurs, the syscall arguments are written into the
	/// `exec_buffer` via [`PassPointerAndWrite`].
	fn execute(
		&mut self,
		instance_id: u32,
		function: PassFatPointerAndRead<&str>,
		gas_left: i64,
		exec_buffer: PassPointerAndWrite<&mut ExecBuffer, { EXEC_BUFFER_SIZE }>,
	) -> ConvertAndReturnAs<Result<u8, ExecError>, RIIntResult<u8, RIExecError>, i64> {
		let instance_id = sp_wasm_interface::InstanceId(instance_id);
		self.virtualization()
			.run(instance_id, gas_left, sp_wasm_interface::ExecAction::Execute(function))
			.expect("execution failed")
			.map(|outcome| {
				*exec_buffer = ExecBuffer::from_outcome(&outcome);
				ExecStatus::from_outcome(&outcome).into()
			})
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// Resume execution after a syscall.
	///
	/// Returns [`ExecStatus::Finished`] or [`ExecStatus::Syscall`] as `u8`.
	/// When a syscall occurs, the syscall arguments are written into the
	/// `exec_buffer` via [`PassPointerAndWrite`].
	fn resume(
		&mut self,
		instance_id: u32,
		gas_left: i64,
		return_value: u64,
		exec_buffer: PassPointerAndWrite<&mut ExecBuffer, { EXEC_BUFFER_SIZE }>,
	) -> ConvertAndReturnAs<Result<u8, ExecError>, RIIntResult<u8, RIExecError>, i64> {
		let instance_id = sp_wasm_interface::InstanceId(instance_id);
		self.virtualization()
			.run(instance_id, gas_left, sp_wasm_interface::ExecAction::Resume(return_value))
			.expect("resume failed")
			.map(|outcome| {
				*exec_buffer = ExecBuffer::from_outcome(&outcome);
				ExecStatus::from_outcome(&outcome).into()
			})
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// Destroy this instance.
	///
	/// Any attempt accessing an instance after destruction will yield the `InvalidInstance` error.
	fn destroy(
		&mut self,
		instance_id: u32,
	) -> ConvertAndReturnAs<Result<(), DestroyError>, RIIntResult<VoidResult, RIDestroyError>, i64>
	{
		let instance_id = sp_wasm_interface::InstanceId(instance_id);
		self.virtualization()
			.destroy(instance_id)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// See `sp_virtualization::Memory::read`.
	fn read_memory(
		&mut self,
		instance_id: u32,
		offset: u32,
		dest: PassFatPointerAndWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Result<(), MemoryError>, RIIntResult<VoidResult, RIMemoryError>, i64> {
		let instance_id = sp_wasm_interface::InstanceId(instance_id);
		self.virtualization()
			.read_memory(instance_id, offset, dest)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// See `sp_virtualization::Memory::write`.
	fn write_memory(
		&mut self,
		instance_id: u32,
		offset: u32,
		src: PassFatPointerAndRead<&[u8]>,
	) -> ConvertAndReturnAs<Result<(), MemoryError>, RIIntResult<VoidResult, RIMemoryError>, i64> {
		let instance_id = sp_wasm_interface::InstanceId(instance_id);
		self.virtualization()
			.write_memory(instance_id, offset, src)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}
}
