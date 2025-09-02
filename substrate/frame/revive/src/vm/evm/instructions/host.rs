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

use crate::{
	vec,
	vm::{evm::U256Converter, Ext},
	Key, RuntimeCosts, LOG_TARGET,
};
use revm::{
	interpreter::{
		gas::{self},
		host::Host,
		interpreter_types::{InputsTr, RuntimeFlag, StackTr},
		InstructionResult,
	},
	primitives::{Bytes, Log, LogData, B256, BLOCK_HASH_HISTORY, U256},
};

/// Implements the BALANCE instruction.
///
/// Gets the balance of the given account.
pub fn balance<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BalanceOf);
	popn_top!([], top, context.interpreter);
	let h160 = sp_core::H160::from_slice(&top.to_be_bytes::<32>()[12..]);
	*top = context.interpreter.extend.balance_of(&h160).into_revm_u256();
}

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn selfbalance<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Balance);
	let balance = context.interpreter.extend.balance();
	push!(context.interpreter, balance.into_revm_u256());
}

/// Implements the EXTCODESIZE instruction.
///
/// Gets the size of an account's code.
pub fn extcodesize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], top, context.interpreter);
	gas!(context.interpreter, RuntimeCosts::CodeSize);
	let h160 = sp_core::H160::from_slice(&top.to_be_bytes::<32>()[12..]);
	let code_size = context.interpreter.extend.code_size(&h160);
	*top = U256::from(code_size);
}

/// EIP-1052: EXTCODEHASH opcode
pub fn extcodehash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], top, context.interpreter);
	gas!(context.interpreter, RuntimeCosts::CodeHash);
	let h160 = sp_core::H160::from_slice(&top.to_be_bytes::<32>()[12..]);
	let code_hash = context.interpreter.extend.code_hash(&h160);
	*top = U256::from_be_bytes(code_hash.0);
}

/// Implements the EXTCODECOPY instruction.
///
/// Copies a portion of an account's code to memory.
pub fn extcodecopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([address, memory_offset, code_offset, len_u256], context.interpreter);

	let h160 = sp_core::H160::from_slice(&address.to_be_bytes::<32>()[12..]);
	let code_hash = context.interpreter.extend.code_hash(&h160);

	let Ok(memory_len): Result<usize, _> = len_u256.try_into() else {
		context.interpreter.halt(InstructionResult::InvalidOperandOOG);
		return;
	};
	let Ok(memory_offset) = memory_offset.try_into() else {
		context.interpreter.halt(InstructionResult::InvalidOperandOOG);
		return;
	};

	let Some(code) = crate::PristineCode::<E::T>::get(&code_hash) else {
		let zeros = vec![0u8; memory_len];
		context.interpreter.memory.set_data(memory_offset, 0, memory_len, &zeros);
		return;
	};
	let Ok(code_offset) = code_offset.try_into() else {
		context.interpreter.halt(InstructionResult::InvalidOperandOOG);
		return;
	};
	if code_offset >= code.len() {
		context.interpreter.halt(InstructionResult::InvalidOperandOOG);
		return;
	};
	// TODO: this needs a new benchmark since we read from DB and copy to memory
	// gas!(context.interpreter, RuntimeCosts::CallDataCopy(memory_len as u32));

	context
		.interpreter
		.memory
		.set_data(memory_offset, code_offset, memory_len, &code);
	// Zero remaining bytes if any
	let available_bytes = if code_offset >= code.len() { 0 } else { code.len() - code_offset };
	let copy_len = memory_len.min(available_bytes);
	if memory_len > copy_len {
		let zero_start = memory_offset + copy_len;
		let zero_len = memory_len - copy_len;
		let zeros = vec![0u8; zero_len];
		context.interpreter.memory.set_data(zero_start, 0, zero_len, &zeros);
	}
}

/// Implements the BLOCKHASH instruction.
///
/// Gets the hash of one of the 256 most recent complete blocks.
pub fn blockhash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockHash);
	popn_top!([], number, context.interpreter);
	let requested_number = <sp_core::U256 as U256Converter>::from_revm_u256(&number);

	let block_number = context.interpreter.extend.block_number();

	let Some(diff) = block_number.checked_sub(requested_number) else {
		*number = U256::ZERO;
		return;
	};

	let diff = if diff > sp_core::U256::from(u64::MAX) { u64::MAX } else { diff.low_u64() };

	// blockhash should push zero if number is same as current block number.
	if diff == 0 {
		*number = U256::ZERO;
		return;
	}

	*number = if diff <= BLOCK_HASH_HISTORY {
		let Some(hash) = context.interpreter.extend.block_hash(requested_number) else {
			context.interpreter.halt(InstructionResult::FatalExternalError);
			return;
		};
		U256::from_be_bytes(hash.0)
	} else {
		U256::ZERO
	}
}

/// Implements the SLOAD instruction.
///
/// Loads a word from storage.
pub fn sload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], index, context.interpreter);
	// NB: SLOAD loads 32 bytes from storage (i.e. U256).
	gas!(context.interpreter, RuntimeCosts::GetStorage(32));
	let key = Key::Fix(index.to_be_bytes());
	let value = context.interpreter.extend.get_storage(&key);

	*index = if let Some(storage_value) = value {
		if storage_value.len() != 32 {
			// sload always reads a word
			context.interpreter.halt(InstructionResult::FatalExternalError);
			return;
		}
		let mut bytes = [0u8; 32];
		bytes.copy_from_slice(&storage_value);
		U256::from_be_bytes(bytes)
	} else {
		// the key was never written before
		U256::ZERO
	};
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([index, value], context.interpreter);

	// Charge gas before set_storage and later adjust it down to the true gas cost
	let Ok(charged_amount) = context
		.interpreter
		.extend
		.gas_meter_mut()
		.charge(RuntimeCosts::SetStorage { new_bytes: 32, old_bytes: 0 })
	else {
		context.interpreter.halt(InstructionResult::OutOfGas);
		return;
	};

	let key = Key::Fix(index.to_be_bytes());
	let take_old = false;
	let Ok(write_outcome) = context.interpreter.extend.set_storage(
		&key,
		Some(value.to_be_bytes::<32>().to_vec()),
		take_old,
	) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	context.interpreter.extend.gas_meter_mut().adjust_gas(
		charged_amount,
		RuntimeCosts::SetStorage { new_bytes: 32, old_bytes: write_outcome.old_len() },
	);
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([index, value], context.interpreter);

	// Charge gas before set_storage and later adjust it down to the true gas cost
	let Ok(charged_amount) = context
		.interpreter
		.extend
		.gas_meter_mut()
		.charge(RuntimeCosts::SetTransientStorage { new_bytes: 32, old_bytes: 0 })
	else {
		context.interpreter.halt(InstructionResult::OutOfGas);
		return;
	};

	let key = Key::Fix(index.to_be_bytes());
	let take_old = false;
	let Ok(write_outcome) = context.interpreter.extend.set_transient_storage(
		&key,
		Some(value.to_be_bytes::<32>().to_vec()),
		take_old,
	) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	context.interpreter.extend.gas_meter_mut().adjust_gas(
		charged_amount,
		RuntimeCosts::SetTransientStorage { new_bytes: 32, old_bytes: write_outcome.old_len() },
	);
}

/// EIP-1153: Transient storage opcodes
/// Load value from transient storage
pub fn tload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], index, context.interpreter);
	gas!(context.interpreter, RuntimeCosts::GetTransientStorage(32));

	let key = Key::Fix(index.to_be_bytes());
	let bytes = context.interpreter.extend.get_transient_storage(&key);
	*index = if let Some(storage_value) = bytes {
		if storage_value.len() != 32 {
			// tload always reads a word
			context.interpreter.halt(InstructionResult::FatalExternalError);
			return;
		}
		let mut bytes = [0u8; 32];
		bytes.copy_from_slice(&storage_value);
		U256::from_be_bytes(bytes)
	} else {
		// the key was never written before
		U256::ZERO
	};
}

/// Implements the LOG0-LOG4 instructions.
///
/// Appends log record with N topics.
pub fn log<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	gas_or_fail_legacy!(context.interpreter, gas::log_cost(N as u8, len as u64));
	let data = if len == 0 {
		Bytes::new()
	} else {
		let offset = as_usize_or_fail!(context.interpreter, offset);
		resize_memory!(context.interpreter, offset, len);
		Bytes::copy_from_slice(context.interpreter.memory.slice_len(offset, len).as_ref())
	};
	if context.interpreter.stack.len() < N {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	}
	let Some(topics) = <_ as StackTr>::popn::<N>(&mut context.interpreter.stack) else {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	};

	let log = Log {
		address: context.interpreter.input.target_address(),
		data: LogData::new(topics.into_iter().map(B256::from).collect(), data)
			.expect("LogData should have <=4 topics"),
	};

	context.host.log(log);
}

/// Implements the SELFDESTRUCT instruction.
///
/// Halt execution and register account for later deletion.
pub fn selfdestruct<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	// TODO: for now this instruction is not supported
	context.interpreter.halt(InstructionResult::NotActivated);

	// Check if we're in a static context
	// require_non_staticcall!(context.interpreter);
	// popn!([beneficiary], context.interpreter);
	// let h160 = sp_core::H160::from_slice(&beneficiary.to_be_bytes::<32>()[12..]);
	// let dispatch_result = context.interpreter.extend.selfdestruct(&h160);

	// match dispatch_result {
	// 	Ok(_) => {
	// 		context.interpreter.halt(InstructionResult::SelfDestruct);
	// 		return;
	// 	},
	// 	Err(e) => {
	// 		log::debug!(target: LOG_TARGET, "Selfdestruct failed: {:?}", e);
	// 		context.interpreter.halt(InstructionResult::FatalExternalError);
	// 		return;
	// 	},
	// }
}
