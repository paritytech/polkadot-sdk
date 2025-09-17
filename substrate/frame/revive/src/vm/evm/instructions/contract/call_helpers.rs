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
	precompiles::{All as AllPrecompiles, Precompiles},
	vm::{evm::U256Converter, Ext},
	Pallet, RuntimeCosts,
};
use core::ops::Range;
use revm::{
	interpreter::{
		interpreter_action::CallScheme,
		interpreter_types::{MemoryTr, StackTr},
		Interpreter,
	},
	primitives::{Address, U256},
};
use sp_core::H160;

/// Gets memory input and output ranges for call instructions.
#[inline]
pub fn get_memory_input_and_out_ranges<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
) -> Option<(Range<usize>, Range<usize>)> {
	popn!([in_offset, in_len, out_offset, out_len], interpreter, None);

	let mut in_range = resize_memory(interpreter, in_offset, in_len)?;

	if !in_range.is_empty() {
		let offset = <_ as MemoryTr>::local_memory_offset(&interpreter.memory);
		in_range = in_range.start.saturating_add(offset)..in_range.end.saturating_add(offset);
	}

	let ret_range = resize_memory(interpreter, out_offset, out_len)?;
	Some((in_range, ret_range))
}

/// Resize memory and return range of memory.
/// If `len` is 0 dont touch memory and return `usize::MAX` as offset and 0 as length.
#[inline]
pub fn resize_memory<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	offset: U256,
	len: U256,
) -> Option<Range<usize>> {
	let len = as_usize_or_fail_ret!(interpreter, len, None);
	let offset = if len != 0 {
		let offset = as_usize_or_fail_ret!(interpreter, offset, None);
		resize_memory!(interpreter, offset, len, None);
		offset
	} else {
		usize::MAX //unrealistic value so we are sure it is not used
	};
	Some(offset..offset + len)
}

/// Calculates gas cost and limit for call instructions.
#[inline]
pub fn calc_call_gas<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	callee: Address,
	scheme: CallScheme,
	input_len: usize,
	value: U256,
) -> Option<u64> {
	let callee: H160 = callee.0 .0.into();
	let precompile = <AllPrecompiles<E::T>>::get::<E>(&callee.as_fixed_bytes());

	match precompile {
		Some(precompile) => {
			// Base cost depending on contract info
			let base_cost = if precompile.has_contract_info() {
				RuntimeCosts::PrecompileWithInfoBase
			} else {
				RuntimeCosts::PrecompileBase
			};
			gas!(interpreter, base_cost, None);

			// Cost for decoding input
			gas!(interpreter, RuntimeCosts::PrecompileDecode(input_len as u32), None);
		},
		None => {
			// Regular CALL / DELEGATECALL base cost / CALLCODE not supported
			let base_cost = if scheme.is_delegate_call() {
				RuntimeCosts::DelegateCallBase
			} else {
				RuntimeCosts::CallBase
			};
			gas!(interpreter, base_cost, None);

			gas!(interpreter, RuntimeCosts::CopyFromContract(input_len as u32), None);
		},
	};
	if !value.is_zero() {
		gas!(
			interpreter,
			RuntimeCosts::CallTransferSurcharge {
				dust_transfer: Pallet::<E::T>::has_dust(crate::U256::from_revm_u256(&value)),
			},
			None
		);
	}
	Some(u64::MAX) // TODO: Set the right gas limit
}
