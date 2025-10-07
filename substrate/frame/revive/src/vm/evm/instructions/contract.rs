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

mod call_helpers;

use super::utility::IntoAddress;
use crate::{
	vm::{
		evm::{interpreter::Halt, util::as_usize_or_halt, Interpreter},
		Ext, RuntimeCosts,
	},
	Code, Error, Pallet, Weight, H160, LOG_TARGET, U256,
};
use alloc::{vec, vec::Vec};
pub use call_helpers::{calc_call_gas, get_memory_in_and_out_ranges};
use core::{
	cmp::min,
	ops::{ControlFlow, Range},
};
use revm::interpreter::interpreter_action::CallScheme;

/// Implements the CREATE/CREATE2 instruction.
///
/// Creates a new contract with provided bytecode.
pub fn create<const IS_CREATE2: bool, E: Ext>(
	interpreter: &mut Interpreter<E>,
) -> ControlFlow<Halt> {
	if interpreter.ext.is_read_only() {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}

	let [value, code_offset, len] = interpreter.stack.popn()?;
	let len = as_usize_or_halt::<E::T>(len)?;

	// TODO: We do not charge for the new code in storage. When implementing the new gas:
	// Introduce EthInstantiateWithCode, which shall charge gas based on the code length.
	// See #9577 for more context.
	interpreter.ext.charge_or_halt(RuntimeCosts::Instantiate {
		input_data_len: len as u32, // We charge for initcode execution
		balance_transfer: Pallet::<E::T>::has_balance(value),
		dust_transfer: Pallet::<E::T>::has_dust(value),
	})?;

	let mut code = Vec::new();
	if len != 0 {
		// EIP-3860: Limit initcode
		if len > revm::primitives::eip3860::MAX_INITCODE_SIZE {
			return ControlFlow::Break(Error::<E::T>::BlobTooLarge.into());
		}

		let code_offset = as_usize_or_halt::<E::T>(code_offset)?;
		interpreter.memory.resize(code_offset, len)?;
		code = interpreter.memory.slice_len(code_offset, len).to_vec();
	}

	let salt = if IS_CREATE2 {
		let [salt] = interpreter.stack.popn()?;
		Some(salt.to_big_endian())
	} else {
		None
	};

	let call_result = interpreter.ext.instantiate(
		Weight::from_parts(u64::MAX, u64::MAX), // TODO: set the right limit
		U256::MAX,
		Code::Upload(code),
		value,
		vec![],
		salt.as_ref(),
	);

	match call_result {
		Ok(address) => {
			let return_value = interpreter.ext.last_frame_output();
			if return_value.did_revert() {
				// Contract creation reverted — return data must be propagated
				interpreter.stack.push(U256::zero())
			} else {
				// Otherwise clear it. Note that RETURN opcode should abort.
				*interpreter.ext.last_frame_output_mut() = Default::default();
				interpreter.stack.push(address)
			}
		},
		Err(err) => {
			log::debug!(target: LOG_TARGET, "Create failed: {err:?}");
			interpreter.stack.push(U256::zero())?;
			ControlFlow::Continue(())
		},
	}
}

/// Implements the CALL instruction.
///
/// Message call with value transfer to another account.
pub fn call<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [_local_gas_limit, to, value] = interpreter.stack.popn()?;
	let to = to.into_address();
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.

	let has_transfer = !value.is_zero();
	if interpreter.ext.is_read_only() && has_transfer {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}

	let (input, return_memory_range) = get_memory_in_and_out_ranges(interpreter)?;
	let scheme = CallScheme::Call;
	let gas_limit = calc_call_gas(interpreter, to, scheme, input.len(), value)?;

	run_call(
		interpreter,
		to,
		interpreter.memory.slice(input).to_vec(),
		scheme,
		Weight::from_parts(gas_limit, u64::MAX),
		value,
		return_memory_range,
	)
}

/// Implements the CALLCODE instruction.
///
/// Message call with alternative account's code.
///
/// Isn't supported yet: [`solc` no longer emits it since Solidity v0.3.0 in 2016]
/// (https://soliditylang.org/blog/2016/03/11/solidity-0.3.0-release-announcement/).
pub fn call_code<E: Ext>(_interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	ControlFlow::Break(Error::<E::T>::InvalidInstruction.into())
}

/// Implements the DELEGATECALL instruction.
///
/// Message call with alternative account's code but same sender and value.
pub fn delegate_call<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [_local_gas_limit, to] = interpreter.stack.popn()?;
	let to = to.into_address();
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.

	let (input, return_memory_range) = get_memory_in_and_out_ranges(interpreter)?;
	let scheme = CallScheme::DelegateCall;
	let value = U256::zero();
	let gas_limit = calc_call_gas(interpreter, to, scheme, input.len(), value)?;

	run_call(
		interpreter,
		to,
		interpreter.memory.slice(input).to_vec(),
		scheme,
		Weight::from_parts(gas_limit, u64::MAX),
		value,
		return_memory_range,
	)
}

/// Implements the STATICCALL instruction.
///
/// Static message call (cannot modify state).
pub fn static_call<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [_local_gas_limit, to] = interpreter.stack.popn()?;
	let to = to.into_address();
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.
	let (input, return_memory_range) = get_memory_in_and_out_ranges(interpreter)?;
	let scheme = CallScheme::StaticCall;
	let value = U256::zero();
	let gas_limit = calc_call_gas(interpreter, to, scheme, input.len(), value)?;

	run_call(
		interpreter,
		to,
		interpreter.memory.slice(input).to_vec(),
		scheme,
		Weight::from_parts(gas_limit, u64::MAX),
		value,
		return_memory_range,
	)
}

fn run_call<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	callee: H160,
	input: Vec<u8>,
	scheme: CallScheme,
	gas_limit: Weight,
	value: U256,
	return_memory_range: Range<usize>,
) -> ControlFlow<Halt> {
	let call_result = match scheme {
		CallScheme::Call | CallScheme::StaticCall => interpreter.ext.call(
			gas_limit,
			U256::MAX,
			&callee,
			value,
			input,
			true,
			scheme.is_static_call(),
		),
		CallScheme::DelegateCall =>
			interpreter.ext.delegate_call(gas_limit, U256::MAX, callee, input),
		CallScheme::CallCode => {
			unreachable!()
		},
	};

	match call_result {
		Ok(()) => {
			let mem_start = return_memory_range.start;
			let mem_length = return_memory_range.len();
			let returned_len = interpreter.ext.last_frame_output().data.len();
			let target_len = min(mem_length, returned_len);

			// success or revert
			interpreter
				.ext
				.gas_meter_mut()
				.charge_or_halt(RuntimeCosts::CopyToContract(target_len as u32))?;

			let return_value = interpreter.ext.last_frame_output();
			let return_data = &return_value.data;
			let did_revert = return_value.did_revert();

			// Note: This can't panic because we resized memory with `get_memory_in_and_out_ranges`
			interpreter.memory.set(mem_start, &return_data[..target_len]);
			interpreter.stack.push(U256::from(!did_revert as u8))
		},
		Err(err) => {
			log::debug!(target: LOG_TARGET, "Call failed: {err:?}");
			interpreter.stack.push(U256::zero())?;
			ControlFlow::Continue(())
		},
	}
}
