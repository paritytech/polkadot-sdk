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

use super::{utility::IntoAddress, Context};
use crate::{
	vm::{evm::U256Converter, Ext, RuntimeCosts},
	Pallet,
};
use alloc::boxed::Box;
pub use call_helpers::{calc_call_gas, get_memory_input_and_out_ranges};
use revm::{
	context_interface::CreateScheme,
	interpreter::{
		gas as revm_gas,
		interpreter_action::{
			CallInputs, CallScheme, CallValue, CreateInputs, FrameInput, InterpreterAction,
		},
		interpreter_types::{LoopControl, RuntimeFlag, StackTr},
		CallInput, InstructionResult,
	},
	primitives::{Address, Bytes, B256, U256},
};

/// Implements the CREATE/CREATE2 instruction.
///
/// Creates a new contract with provided bytecode.
pub fn create<'ext, const IS_CREATE2: bool, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([value, code_offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);

	// TODO: We do not charge for the new code in storage. When implementing the new gas:
	// Introduce EthInstantiateWithCode, which shall charge gas based on the code length.
	// See #9577 for more context.
	let val = crate::U256::from_revm_u256(&value);
	gas!(
		context.interpreter,
		RuntimeCosts::Instantiate {
			input_data_len: len as u32, // We charge for initcode execution
			balance_transfer: Pallet::<E::T>::has_balance(val),
			dust_transfer: Pallet::<E::T>::has_dust(val),
		}
	);

	let mut code = Bytes::new();
	if len != 0 {
		// EIP-3860: Limit initcode
		if len > revm::primitives::eip3860::MAX_INITCODE_SIZE {
			context.interpreter.halt(InstructionResult::CreateInitCodeSizeLimit);
			return;
		}

		let code_offset = as_usize_or_fail!(context.interpreter, code_offset);
		resize_memory!(context.interpreter, code_offset, len);
		code =
			Bytes::copy_from_slice(context.interpreter.memory.slice_len(code_offset, len).as_ref());
	}

	// EIP-1014: Skinny CREATE2
	let scheme = if IS_CREATE2 {
		popn!([salt], context.interpreter);
		CreateScheme::Create2 { salt }
	} else {
		gas_legacy!(context.interpreter, revm_gas::CREATE);
		CreateScheme::Create
	};

	// Call host to interact with target contract
	context
		.interpreter
		.bytecode
		.set_action(InterpreterAction::NewFrame(FrameInput::Create(Box::new(CreateInputs {
			caller: context.interpreter.extend.address().0.into(),
			scheme,
			value,
			init_code: code,
			gas_limit: u64::MAX, // TODO: set the right limit
		}))));
}

/// Implements the CALL instruction.
///
/// Message call with value transfer to another account.
pub fn call<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([local_gas_limit, to, value], context.interpreter);
	let to = to.into_address();
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.
	let _local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

	let has_transfer = !value.is_zero();
	if context.interpreter.runtime_flag.is_static() && has_transfer {
		context.interpreter.halt(InstructionResult::CallNotAllowedInsideStatic);
		return;
	}

	let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(context.interpreter)
	else {
		return;
	};

	let scheme = CallScheme::Call;
	let input = CallInput::SharedBuffer(input);

	let Some(gas_limit) = calc_call_gas(context.interpreter, to, scheme, input.len(), value) else {
		return;
	};

	// Call host to interact with target contract
	context
		.interpreter
		.bytecode
		.set_action(InterpreterAction::NewFrame(FrameInput::Call(Box::new(CallInputs {
			input,
			gas_limit,
			target_address: to,
			caller: Address::default(),
			bytecode_address: to,
			value: CallValue::Transfer(value),
			scheme,
			is_static: context.interpreter.runtime_flag.is_static(),
			return_memory_offset,
		}))));
}

/// Implements the CALLCODE instruction.
///
/// Message call with alternative account's code.
///
/// Isn't supported yet: [`solc` no longer emits it since Solidity v0.3.0 in 2016]
/// (https://soliditylang.org/blog/2016/03/11/solidity-0.3.0-release-announcement/).
pub fn call_code<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(revm::interpreter::InstructionResult::NotActivated);
}

/// Implements the DELEGATECALL instruction.
///
/// Message call with alternative account's code but same sender and value.
pub fn delegate_call<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([local_gas_limit, to], context.interpreter);
	let to = Address::from_word(B256::from(to));
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.
	let _local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

	let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(context.interpreter)
	else {
		return;
	};

	let scheme = CallScheme::DelegateCall;
	let input = CallInput::SharedBuffer(input);

	let Some(gas_limit) = calc_call_gas(context.interpreter, to, scheme, input.len(), U256::ZERO)
	else {
		return;
	};

	// Call host to interact with target contract
	context
		.interpreter
		.bytecode
		.set_action(InterpreterAction::NewFrame(FrameInput::Call(Box::new(CallInputs {
			input,
			gas_limit,
			target_address: Default::default(),
			caller: Default::default(),
			bytecode_address: to,
			value: CallValue::Apparent(Default::default()),
			scheme,
			is_static: context.interpreter.runtime_flag.is_static(),
			return_memory_offset,
		}))));
}

/// Implements the STATICCALL instruction.
///
/// Static message call (cannot modify state).
pub fn static_call<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([local_gas_limit, to], context.interpreter);
	let to = Address::from_word(B256::from(to));
	// TODO: Max gas limit is not possible in a real Ethereum situation. This issue will be
	// addressed in #9577.
	let _local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

	let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(context.interpreter)
	else {
		return;
	};

	let scheme = CallScheme::StaticCall;
	let input = CallInput::SharedBuffer(input);

	let Some(gas_limit) = calc_call_gas(context.interpreter, to, scheme, input.len(), U256::ZERO)
	else {
		return;
	};

	// Call host to interact with target contract
	context
		.interpreter
		.bytecode
		.set_action(InterpreterAction::NewFrame(FrameInput::Call(Box::new(CallInputs {
			input,
			gas_limit,
			target_address: to,
			caller: Default::default(),
			bytecode_address: to,
			value: CallValue::Transfer(U256::ZERO),
			scheme,
			is_static: true,
			return_memory_offset,
		}))));
}
