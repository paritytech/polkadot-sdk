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
	vm::{
		evm::{interpreter::Halt, util::as_usize_or_halt, Interpreter},
		Ext,
	},
	Pallet, RuntimeCosts,
};
use core::ops::{ControlFlow, Range};
use revm::interpreter::interpreter_action::CallScheme;
use sp_core::{H160, U256};

/// Gets memory input and output ranges for call instructions.
pub fn get_memory_in_and_out_ranges<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
) -> ControlFlow<Halt, (Range<usize>, Range<usize>)> {
	let [in_offset, in_len, out_offset, out_len] = interpreter.stack.popn()?;
	let in_range = resize_memory(interpreter, in_offset, in_len)?;
	let ret_range = resize_memory(interpreter, out_offset, out_len)?;
	ControlFlow::Continue((in_range, ret_range))
}

/// Resize memory and return range of memory.
/// If `len` is 0 dont touch memory and return `usize::MAX` as offset and 0 as length.
pub fn resize_memory<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	offset: U256,
	len: U256,
) -> ControlFlow<Halt, Range<usize>> {
	let len = as_usize_or_halt::<E::T>(len)?;
	if len != 0 {
		let offset = as_usize_or_halt::<E::T>(offset)?;
		interpreter.memory.resize(offset, len)?;
		ControlFlow::Continue(offset..offset + len)
	} else {
		//unrealistic value so we are sure it is not used
		ControlFlow::Continue(usize::MAX..usize::MAX)
	}
}

/// Calculates gas cost and limit for call instructions.
pub fn calc_call_gas<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	callee: H160,
	scheme: CallScheme,
	input_len: usize,
	value: U256,
) -> ControlFlow<Halt, u64> {
	let precompile = <AllPrecompiles<E::T>>::get::<E>(&callee.as_fixed_bytes());

	match precompile {
		Some(precompile) => {
			// Base cost depending on contract info
			interpreter
				.ext
				.gas_meter_mut()
				.charge_or_halt(if precompile.has_contract_info() {
					RuntimeCosts::PrecompileWithInfoBase
				} else {
					RuntimeCosts::PrecompileBase
				})?;

			// Cost for decoding input
			interpreter
				.ext
				.gas_meter_mut()
				.charge_or_halt(RuntimeCosts::PrecompileDecode(input_len as u32))?;
		},
		None => {
			// Regular CALL / DELEGATECALL base cost / CALLCODE not supported
			interpreter.ext.charge_or_halt(if scheme.is_delegate_call() {
				RuntimeCosts::DelegateCallBase
			} else {
				RuntimeCosts::CallBase
			})?;

			interpreter
				.ext
				.gas_meter_mut()
				.charge_or_halt(RuntimeCosts::CopyFromContract(input_len as u32))?;
		},
	};
	if !value.is_zero() {
		interpreter
			.ext
			.gas_meter_mut()
			.charge_or_halt(RuntimeCosts::CallTransferSurcharge {
				dust_transfer: Pallet::<E::T>::has_dust(value),
			})?;
	}

	ControlFlow::Continue(u64::MAX) // TODO: Set the right gas limit
}
