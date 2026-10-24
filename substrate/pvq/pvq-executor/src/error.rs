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

//! Defines error types for PVQ executor.
use pvq_primitives::PvqError;
/// Errors that can occur while executing a PVQ program.
#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum PvqExecutorError<UserError> {
	/// The program format is invalid.
	#[cfg_attr(feature = "std", error("Invalid PVQ program format"))]
	InvalidProgramFormat,
	/// A memory access error occurred.
	#[cfg_attr(feature = "std", error("Memory access error: {0}"))]
	MemoryAccessError(polkavm::MemoryAccessError),
	/// A trap occurred during execution.
	#[cfg_attr(feature = "std", error("Trap"))]
	Trap,
	/// Not enough gas to execute the program.
	#[cfg_attr(feature = "std", error("Not enough gas"))]
	NotEnoughGas,
	/// A user-defined error occurred.
	#[cfg_attr(feature = "std", error("User error: {0}"))]
	User(UserError),
	/// A step error occurred.
	#[cfg_attr(feature = "std", error("Execution stepped"))]
	Step,
	/// Another error from PolkaVM occurred.
	#[cfg_attr(feature = "std", error("Other PVM error: {0}"))]
	OtherPvmError(polkavm::Error),
}

impl<UserError> From<polkavm::CallError<UserError>> for PvqExecutorError<UserError> {
	fn from(err: polkavm::CallError<UserError>) -> Self {
		match err {
			polkavm::CallError::Trap => Self::Trap,
			polkavm::CallError::NotEnoughGas => Self::NotEnoughGas,
			polkavm::CallError::Error(e) => Self::OtherPvmError(e),
			polkavm::CallError::Step => Self::Step,
			polkavm::CallError::User(e) => Self::User(e),
		}
	}
}

impl<UserError> From<polkavm::Error> for PvqExecutorError<UserError> {
	fn from(e: polkavm::Error) -> Self {
		Self::OtherPvmError(e)
	}
}

impl<UserError> From<polkavm::MemoryAccessError> for PvqExecutorError<UserError> {
	fn from(e: polkavm::MemoryAccessError) -> Self {
		Self::MemoryAccessError(e)
	}
}

#[cfg(feature = "std")]
impl<UserError: core::fmt::Debug> From<PvqExecutorError<UserError>> for PvqError {
	fn from(e: PvqExecutorError<UserError>) -> PvqError {
		match e {
			PvqExecutorError::InvalidProgramFormat => "Invalid PVQ program format".to_string(),
			PvqExecutorError::MemoryAccessError(_) => "Memory access error".to_string(),
			PvqExecutorError::Trap => "Trap".to_string(),
			PvqExecutorError::NotEnoughGas => "Not enough gas".to_string(),
			PvqExecutorError::User(user_error) => format!("Host call error: {user_error:?}"),
			PvqExecutorError::OtherPvmError(pvm_error) => format!("Other error: {pvm_error:?}"),
			PvqExecutorError::Step => "Execution stepped".to_string(),
		}
	}
}

#[cfg(not(feature = "std"))]
impl<UserError> From<PvqExecutorError<UserError>> for PvqError {
	fn from(e: PvqExecutorError<UserError>) -> PvqError {
		match e {
			PvqExecutorError::InvalidProgramFormat => PvqError::InvalidPvqProgramFormat,
			PvqExecutorError::MemoryAccessError(_) => PvqError::MemoryAccessError,
			PvqExecutorError::Trap => PvqError::Trap,
			PvqExecutorError::NotEnoughGas => PvqError::QueryExceedsWeightLimit,
			PvqExecutorError::User(_) => PvqError::HostCallError,
			PvqExecutorError::Step => PvqError::Step,
			PvqExecutorError::OtherPvmError(_) => PvqError::Other,
		}
	}
}
