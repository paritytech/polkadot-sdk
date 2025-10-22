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
use crate::{vm::evm::interpreter::Halt, Config, Error, U256};
use core::ops::ControlFlow;

/// Helper function to convert U256 to usize, checking for overflow
pub fn as_usize_or_halt_with(value: U256, halt: impl Fn() -> Halt) -> ControlFlow<Halt, usize> {
	let limbs = value.0;
	if (limbs[0] > usize::MAX as u64) | (limbs[1] != 0) | (limbs[2] != 0) | (limbs[3] != 0) {
		ControlFlow::Break(halt())
	} else {
		ControlFlow::Continue(limbs[0] as usize)
	}
}

/// Helper function to convert U256 to usize, checking for overflow, with default OutOfGas
/// error
pub fn as_usize_or_halt<T: Config>(value: U256) -> ControlFlow<Halt, usize> {
	as_usize_or_halt_with(value, || Error::<T>::OutOfGas.into())
}
