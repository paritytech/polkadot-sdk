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

//! This crate is intended for use by runtime code (e.g pallet-contracts) to spawn PolkaVM instances
//! and execute calls into them. Its purpose is to add one layer of abstraction to that it works
//! transparently from the actual runtime (via the host functions defined in this crate) but also
//! from tests (which run natively).
//!
//! Additionally, this crate is also used (by the executor) to implement the host functions that are
//! defined in this crate. This allows us to encapsulate all the logic regarding PolkaVM setup in
//! one place.
//!
//! Please keep in mind that the interface is kept simple because it has to match the interface
//! of the host function so that the abstraction works. It will never expose the whole PolkaVM
//! interface.
//!
//! # ⚠️ Unstable API — Do Not Use in Production ⚠️
//!
//! **This crate's API is unstable and subject to breaking changes without notice.**
//!
//! The virtualization host functions exposed by this crate have **not been stabilized** and are
//! **not available on Polkadot** (or any other production relay/parachain) until they are. Using
//! them in a production runtime **will cause your runtime to break** when the API changes.
//!
//! This crate should **only** be used for:
//! - Local testing and development
//! - Experimentation on test networks
//!
//! **Do not** ship runtimes that depend on this crate to any chain you care about. There is no
//! stability guarantee and no deprecation period — the interface may change at any time.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(not(feature = "std"))]
mod forwarder;
#[cfg(not(feature = "std"))]
pub use forwarder::Virt;

#[cfg(feature = "std")]
mod manager;
#[cfg(feature = "std")]
mod native;
#[cfg(feature = "std")]
pub use manager::VirtManager;
#[cfg(feature = "std")]
pub use native::Virt;

mod host_functions;
mod tests;

pub use crate::tests::run as run_tests;

pub use crate::host_functions::virtualization as host_fn;

use codec::{Decode, Encode};
use core::mem;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// The concrete memory type used to access the memory of [`Virt`].
pub type Memory = <Virt as VirtT>::Memory;

/// The target we use for all logging.
pub const LOG_TARGET: &str = "virtualization";

// Re-export from sp_wasm_interface so that both the executor and the runtime code
// use the same type.
pub use sp_wasm_interface::{ExecAction, ExecOutcome};

/// Buffer shared between runtime and executor for passing syscall data across the
/// host function boundary.
///
/// The runtime allocates this on its stack and passes it via pointer.
/// The host fills it in when returning from [`VirtT::run`].
#[derive(Debug, Default)]
#[repr(C)]
pub struct ExecBuffer {
	/// Gas remaining after the execution step.
	pub gas_left: i64,
	/// The syscall number (only meaningful when the status is [`ExecStatus::Syscall`]).
	pub syscall_no: u32,
	/// Padding to maintain alignment after syscall_no.
	pub _pad: u32,
	/// Syscall register arguments a0-a5 (only meaningful for [`ExecStatus::Syscall`]).
	pub a0: u64,
	pub a1: u64,
	pub a2: u64,
	pub a3: u64,
	pub a4: u64,
	pub a5: u64,
}

/// The size of [`ExecBuffer`] in bytes.
pub const EXEC_BUFFER_SIZE: usize = mem::size_of::<ExecBuffer>();

impl AsRef<[u8]> for ExecBuffer {
	fn as_ref(&self) -> &[u8] {
		// SAFETY: `ExecBuffer` is `#[repr(C)]` with a well-defined layout of primitive fields.
		unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, EXEC_BUFFER_SIZE) }
	}
}

impl AsMut<[u8]> for ExecBuffer {
	fn as_mut(&mut self) -> &mut [u8] {
		// SAFETY: `ExecBuffer` is `#[repr(C)]` with a well-defined layout of primitive fields.
		unsafe { core::slice::from_raw_parts_mut(self as *mut Self as *mut u8, EXEC_BUFFER_SIZE) }
	}
}

impl ExecBuffer {
	/// Populate this buffer from an [`ExecOutcome`].
	pub fn from_outcome(outcome: &ExecOutcome) -> Self {
		match *outcome {
			ExecOutcome::Finished { gas_left } => Self { gas_left, ..Default::default() },
			ExecOutcome::Syscall { gas_left, syscall_no, a0, a1, a2, a3, a4, a5 } => {
				Self { gas_left, syscall_no, _pad: 0, a0, a1, a2, a3, a4, a5 }
			},
		}
	}

	/// Decode a status byte and this buffer into an [`ExecOutcome`].
	pub fn into_outcome(self, status: ExecStatus) -> ExecOutcome {
		match status {
			ExecStatus::Finished => ExecOutcome::Finished { gas_left: self.gas_left },
			ExecStatus::Syscall => ExecOutcome::Syscall {
				gas_left: self.gas_left,
				syscall_no: self.syscall_no,
				a0: self.a0,
				a1: self.a1,
				a2: self.a2,
				a3: self.a3,
				a4: self.a4,
				a5: self.a5,
			},
		}
	}
}

/// Status returned by the `execute` / `resume` host functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum ExecStatus {
	/// Execution finished normally.
	Finished = 0,
	/// A syscall was encountered — check the [`ExecBuffer`] for details.
	Syscall = 1,
}

impl ExecStatus {
	/// Derive the status from an [`ExecOutcome`].
	pub fn from_outcome(outcome: &ExecOutcome) -> Self {
		match outcome {
			ExecOutcome::Finished { .. } => Self::Finished,
			ExecOutcome::Syscall { .. } => Self::Syscall,
		}
	}
}

/// A virtualization instance that can be called into multiple times.
///
/// There are only two implementations of this trait. One which is used within runtime builds.
/// We call this the `forwarder` since it only forwards the calls to host functions. The other
/// one is the `native` implementation which is used to implement said host functions and is also
/// used by the pallet's test code.
///
/// A trait is not strictly necessary but makes sure that both implementations do not diverge.
///
/// # ⚠️ Unstable — Do Not Use in Production
///
/// This trait and its implementations are **unstable**. The virtualization host functions are
/// **not available on Polkadot** until the API is stabilized. Using them in a production
/// runtime will cause breakage when the API changes. Only use for testing and experimentation.
pub trait VirtT: Sized {
	/// The memory implementation of this instance.
	type Memory: MemoryT;

	/// Compile and instantiate the passed `program`.
	///
	/// The passed program has to be a valid PolkaVM program.
	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError>;

	/// Execute or resume a virtualization instance.
	///
	/// When `action` is [`ExecAction::Execute`], starts executing the named exported
	/// function. The function must not take any arguments nor return any results.
	/// When `action` is [`ExecAction::Resume`], resumes after a syscall with the given
	/// return value (written into register `a0`).
	///
	/// Returns [`ExecOutcome::Finished`] when execution completes or
	/// [`ExecOutcome::Syscall`] when a host function is called. In the latter case,
	/// the caller should handle the syscall and call this method again with
	/// [`ExecAction::Resume`] to continue.
	///
	/// * `gas_left`: How much gas the execution is allowed to consume.
	/// * `action`: Whether to start a new execution or resume an existing one.
	fn run(&mut self, gas_left: i64, action: ExecAction<'_>) -> Result<ExecOutcome, ExecError>;

	/// Get a reference to the instances memory.
	///
	/// Memory access will fail with an error when this instance was destroyed.
	fn memory(&self) -> Self::Memory;
}

/// Allows to access the memory of a [`VirtT`].
pub trait MemoryT {
	/// Read the instances memory at `offset` into `dest`.
	fn read(&mut self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError>;

	/// Write `src` into the instances memory at `offset`.
	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError>;
}

/// Errors that can be emitted when instantiating a new virtualization instance.
#[derive(Encode, Decode, TryFromPrimitive, IntoPrimitive, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstantiateError {
	/// The supplied code was invalid.
	InvalidImage = 1,
}

/// Errors that can be emitted when executing a new virtualization instance.
#[derive(Encode, Decode, TryFromPrimitive, IntoPrimitive, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecError {
	/// The supplied `instance_id` was invalid or the instance was destroyed.
	///
	/// This error will also be returned if a recursive call into the same instance
	/// is attempted.
	InvalidInstance = 1,
	/// The supplied code was invalid. Most likely caused by invalid entry points.
	InvalidImage = 2,
	/// The execution ran out of gas before it could finish.
	OutOfGas = 3,
	/// The execution trapped before it could finish.
	///
	/// This can be caused by executing an `unimp` instruction.
	Trap = 4,
}

/// Errors that can be emitted when accessing a virtualization instance's memory.
#[derive(Encode, Decode, TryFromPrimitive, IntoPrimitive, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MemoryError {
	/// The supplied `instance_id` was invalid or the instance was destroyed.
	InvalidInstance = 1,
	/// The memory region specified is not accessible.
	OutOfBounds = 2,
}

/// Errors that can be emitted when destroying a virtualization instance.
#[derive(Encode, Decode, TryFromPrimitive, IntoPrimitive, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DestroyError {
	/// The supplied `instance_id` was invalid or the instance was destroyed.
	InvalidInstance = 1,
}
