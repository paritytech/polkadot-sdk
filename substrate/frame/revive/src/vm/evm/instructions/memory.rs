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

use super::Context;
use crate::vm::Ext;
use core::cmp::max;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_types::{MemoryTr, RuntimeFlag, StackTr},
	},
	primitives::U256,
};

/// Implements the MLOAD instruction.
///
/// Loads a 32-byte word from memory.
pub fn mload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], top, context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, top);
	resize_memory!(context.interpreter, offset, 32);
	*top =
		U256::try_from_be_slice(context.interpreter.memory.slice_len(offset, 32).as_ref()).unwrap()
}

/// Implements the MSTORE instruction.
///
/// Stores a 32-byte word to memory.
pub fn mstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn!([offset, value], context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, offset);
	resize_memory!(context.interpreter, offset, 32);
	context.interpreter.memory.set(offset, &value.to_be_bytes::<32>());
}

/// Implements the MSTORE8 instruction.
///
/// Stores a single byte to memory.
pub fn mstore8<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn!([offset, value], context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, offset);
	resize_memory!(context.interpreter, offset, 1);
	context.interpreter.memory.set(offset, &[value.byte(0)]);
}

/// Implements the MSIZE instruction.
///
/// Gets the size of active memory in bytes.
pub fn msize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.memory.size()));
}

/// Implements the MCOPY instruction.
///
/// EIP-5656: Memory copying instruction that copies memory from one location to another.
pub fn mcopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	popn!([dst, src, len], context.interpreter);

	// Into usize or fail
	let len = as_usize_or_fail!(context.interpreter, len);
	// Deduce gas
	gas_or_fail!(context.interpreter, revm_gas::copy_cost_verylow(len));
	if len == 0 {
		return;
	}

	let dst = as_usize_or_fail!(context.interpreter, dst);
	let src = as_usize_or_fail!(context.interpreter, src);
	// Resize memory
	resize_memory!(context.interpreter, max(dst, src), len);
	// Copy memory in place
	context.interpreter.memory.copy(dst, src, len);
}
