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
//! transparently from the actual runtime but also from tests (which run natively). Additionally,
//! this crate is also used to implement the host functions that are used by this crate. This allows
//! us to encapsulate all the logic regarding PolkaVM setup in one place.
//!
//! Please keep in mind that the interface is kept simple because it has to match the interface
//! of the host function so that the abstraction works. It will never expose the whole PolkaVM
//! interface.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(not(feature = "std"))]
mod forwarder;
#[cfg(not(feature = "std"))]
pub use forwarder::Virt;

#[cfg(feature = "std")]
mod native;
#[cfg(feature = "std")]
pub use native::Virt;

mod tests;

pub use tests::run as run_tests;

pub use sp_io::{
	VirtDestroyError as DestroyError, VirtExecError as ExecError,
	VirtInstantiateError as InstantiateError, VirtMemoryError as MemoryError,
	VirtSharedState as SharedState, VirtSyscallHandler as SyscallHandler,
};

/// The concrete memory type used to access the memory of [`Virt`].
pub type Memory = <Virt as VirtT>::Memory;

/// The target we use for all logging.
pub const LOG_TARGET: &str = "virtualization";

/// A virtualization instance that can be called into multiple times.
///
/// There are only two implementations of this trait. One which is used within runtime builds.
/// We call this the `forwarder` since it only forwards the calls to host functions. The other
/// one is the `native` implementation which is used to implement said host functions and is also
/// used by the pallet's test code.
///
/// A trait is not strictly necessary but makes sure that both implementations do not diverge.
pub trait VirtT: Sized {
	/// The memory implementation of this instance.
	type Memory: MemoryT;

	/// Compile and instantiate the passed `program`.
	///
	/// The passed program has to be a valid PolkaVM program.
	fn instantiate(program: &[u8]) -> Result<Self, InstantiateError>;

	/// Execute the exported `function`.
	///
	/// The exported function must not take any arguments nor return any results.
	///
	/// * `function`: The identifier of the PolkaVM export.
	/// * `syscall_handler`: Will be called to handle imported functions.
	/// * `state`: This reference will be passed as first argument to the `syscall_handler`. Use to
	///   hold state.
	fn execute<T>(
		&mut self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError>;

	/// Same as [`Self::execute`] but destroys the instance right away.
	///
	/// This is an optimization to allow the `forwarder` implementation to
	/// execute an destroy in a single host function call. Otherwise it would
	/// need to issue another host function call on drop.
	fn execute_and_destroy<T>(
		self,
		function: &str,
		syscall_handler: SyscallHandler<T>,
		state: &mut SharedState<T>,
	) -> Result<(), ExecError>;

	/// Get a reference to the instances memory.
	///
	/// You want to make this part of the [`SharedState`] in order to be able to access
	/// the memory from your syscall handler.
	///
	/// Memory access will fail with an error when this instance was destroyed.
	fn memory(&self) -> Self::Memory;
}

/// Allows to access the memory of a [`VirtT`].
pub trait MemoryT {
	/// Read the instances memory at `offset` into `dest`.
	fn read(&self, offset: u32, dest: &mut [u8]) -> Result<(), MemoryError>;

	/// Write `src` into the instances memory at `offset`.
	fn write(&mut self, offset: u32, src: &[u8]) -> Result<(), MemoryError>;
}
