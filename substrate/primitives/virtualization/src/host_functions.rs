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
use sp_runtime_interface::{
	pass_by::{ConvertAndReturnAs, PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};
use strum::EnumCount;

#[derive(EnumCount)]
#[repr(i8)]
pub enum RIInstantiateError {
	InvalidImage = -1i8,
}

impl From<RIInstantiateError> for i64 {
	fn from(error: RIInstantiateError) -> Self {
		error as i64
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
	InvalidGasValue = -4,
	Trap = -5,
}

impl From<RIExecError> for i64 {
	fn from(error: RIExecError) -> Self {
		error as i64
	}
}

impl From<RIExecError> for ExecError {
	fn from(error: RIExecError) -> Self {
		match error {
			RIExecError::InvalidInstance => ExecError::InvalidInstance,
			RIExecError::InvalidImage => ExecError::InvalidImage,
			RIExecError::OutOfGas => ExecError::OutOfGas,
			RIExecError::InvalidGasValue => ExecError::InvalidGasValue,
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
			ExecError::InvalidGasValue => RIExecError::InvalidGasValue,
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
// ----------8< CUT HERE 8<----------

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

trait LessThan64BitPositiveInteger: Into<i64> {
	const MAX: i64;
}

impl LessThan64BitPositiveInteger for u8 {
	const MAX: i64 = u8::MAX as i64;
}
impl LessThan64BitPositiveInteger for u16 {
	const MAX: i64 = u16::MAX as i64;
}
impl LessThan64BitPositiveInteger for u32 {
	const MAX: i64 = u32::MAX as i64;
}

impl<R: Into<i64> + LessThan64BitPositiveInteger, E: Into<i64> + strum::EnumCount>
	From<RIIntResult<R, E>> for i64
{
	fn from(result: RIIntResult<R, E>) -> Self {
		match result {
			RIIntResult::Ok(value) => value.into(),
			RIIntResult::Err(e) => {
				let error_code: i64 = e.into();
				assert!(
					error_code > 0 && error_code <= E::COUNT as i64,
					"Error variant index out of bounds"
				);
				-error_code
			},
		}
	}
}

impl<R: TryFrom<i64> + LessThan64BitPositiveInteger, E: TryFrom<i64> + strum::EnumCount>
	TryFrom<i64> for RIIntResult<R, E>
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

impl LessThan64BitPositiveInteger for VoidResult {
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

// ----------8< CUT HERE 8<----------

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
	fn instantiate(
		&mut self,
		program: PassFatPointerAndRead<&[u8]>,
	) -> ConvertAndReturnAs<Result<u32, InstantiateError>, RIIntResult<u32, RIInstantiateError>, i64>
	{
		self.virtualization()
			.instantiate(program)
			.expect("instantiation failed")
			.map(|instance_id| instance_id as u32)
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
		function: PassFatPointerAndRead<&str>,
		syscall_handler: u32,
		state_ptr: u32,
		destroy: bool,
	) -> ConvertAndReturnAs<Result<(), ExecError>, RIIntResult<VoidResult, RIExecError>, i64> {
		self.virtualization()
			.execute(instance_id, function, syscall_handler, state_ptr, destroy)
			.expect("execution failed")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}

	/// Destroy this instance.
	///
	/// Any attempt accessing an instance after destruction will yield the `InvalidInstance` error.
	fn destroy(
		&mut self,
		instance_id: u64,
	) -> ConvertAndReturnAs<Result<(), DestroyError>, RIIntResult<VoidResult, RIDestroyError>, i64>
	{
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
		dest: PassFatPointerAndWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Result<(), MemoryError>, RIIntResult<VoidResult, RIMemoryError>, i64> {
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
		src: PassFatPointerAndRead<&[u8]>,
	) -> ConvertAndReturnAs<Result<(), MemoryError>, RIIntResult<VoidResult, RIMemoryError>, i64> {
		self.virtualization()
			.write_memory(instance_id, offset, src)
			.expect("memory access error")
			.map_err(|err| TryFrom::try_from(err).expect("Invalid error"))
	}
}
