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

use super::ExtBytecode;
use crate::{
	primitives::ExecReturnValue,
	vm::{
		evm::{memory::Memory, stack::Stack},
		ExecResult, Ext,
	},
	Config, DispatchError, Error,
};
use alloc::vec::Vec;
use pallet_revive_uapi::ReturnFlags;

/// EVM execution halt - either successful termination or error
#[derive(Debug, PartialEq)]
pub enum Halt {
	Stop,
	Return(Vec<u8>),
	Revert(Vec<u8>),
	Err(DispatchError),
}

impl<T: Config> From<Error<T>> for Halt {
	fn from(err: Error<T>) -> Self {
		Halt::Err(err.into())
	}
}

impl From<Halt> for ExecResult {
	fn from(halt: Halt) -> Self {
		match halt {
			Halt::Stop => Ok(ExecReturnValue::default()),
			Halt::Return(data) => Ok(ExecReturnValue { flags: ReturnFlags::empty(), data }),
			Halt::Revert(data) => Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data }),
			Halt::Err(err) => Err(err.into()),
		}
	}
}

/// EVM interpreter state using sp_core types
#[derive(Debug)]
pub struct Interpreter<'a, E: Ext> {
	/// Access to the environment
	pub ext: &'a mut E,
	/// The bytecode being executed
	pub bytecode: ExtBytecode,
	/// Input data for the current call
	pub input: Vec<u8>, // TODO maybe just &'a[u8]
	/// The execution stack
	pub stack: Stack<E::T>,
	/// EVM memory
	pub memory: Memory<E::T>,
}

impl<'a, E: Ext> Interpreter<'a, E> {
	/// Create a new interpreter instance
	pub fn new(bytecode: ExtBytecode, input: Vec<u8>, ext: &'a mut E) -> Self {
		Self { ext, bytecode, input, stack: Stack::new(), memory: Memory::new() }
	}
}
