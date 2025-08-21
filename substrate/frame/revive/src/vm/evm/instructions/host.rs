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

use crate::{storage::WriteOutcome, vm::Ext, Key, RuntimeCosts, LOG_TARGET};
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
	let balance = context.interpreter.extend.balance_of(&h160);
	let bytes: [u8; 32] = balance.to_big_endian();
	*top = U256::from_be_bytes(bytes);
}

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn selfbalance<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Balance);
	let balance = context.interpreter.extend.balance();
	let bytes: [u8; 32] = balance.to_big_endian();
	let alloy_balance = U256::from_be_bytes(bytes);
	push!(context.interpreter, alloy_balance);
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

	let Some(code) =
		crate::PristineCode::<E::T>::get(&code_hash).map(|bounded_vec| bounded_vec.into_inner())
	else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};

	let Ok(code_offset) = code_offset.try_into() else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
	if code_offset >= code.len() {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
	let Ok(memory_len): Result<usize, _> = len_u256.try_into() else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
	if memory_len > code.len().saturating_sub(code_offset) {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
	let Ok(memory_offset) = memory_offset.try_into() else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};

	// Copy cost: 3 gas per 32-byte word
	let copy_gas = (memory_len.div_ceil(32) * 3) as u32; // Round up to nearest 32-byte boundary
													  // static gas for this instruction 100
													  // gas!(context.interpreter, RuntimeCosts::EVMGas(100+copy_gas));
	gas!(context.interpreter, RuntimeCosts::CallDataCopy(memory_len as u32));

	context
		.interpreter
		.memory
		.set_data(memory_offset, code_offset, memory_len, &code);
}

/// Implements the BLOCKHASH instruction.
///
/// Gets the hash of one of the 256 most recent complete blocks.
pub fn blockhash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockHash);
	popn_top!([], number, context.interpreter);

	let requested_number = {
		let bytes = number.to_be_bytes::<32>();
		sp_core::U256::from_big_endian(&bytes)
	};

	let block_number = context.interpreter.extend.block_number();

	let Some(diff) = block_number.checked_sub(requested_number) else {
		context.interpreter.halt(InstructionResult::Revert);
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
			context.interpreter.halt(InstructionResult::Revert);
			return;
		};
		U256::from_be_bytes(hash.0)
	} else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	}
}

/// Implements the SLOAD instruction.
///
/// Loads a word from storage.
pub fn sload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], index, context.interpreter);
	gas!(context.interpreter, RuntimeCosts::GetStorage(32)); // TODO: correct number here?
	let key = Key::Fix(index.to_be_bytes());
	let value = context.interpreter.extend.get_storage(&key);

	*index = if let Some(storage_value) = value {
		if storage_value.len() != 32 {
			// sload always reads a word
			context.interpreter.halt(InstructionResult::Revert);
			return;
		}
		let mut bytes = [0u8; 32];
		bytes.copy_from_slice(&storage_value);
		U256::from_be_bytes(bytes)
	} else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([index, value], context.interpreter);

	let key = Key::Fix(index.to_be_bytes());
	let take_old = false;
	let Ok(write_outcome) = context.interpreter.extend.set_storage(
		&key,
		Some(value.to_be_bytes::<32>().to_vec()),
		take_old,
	) else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
	match write_outcome {
		WriteOutcome::New => {
			gas!(context.interpreter, RuntimeCosts::SetStorage { old_bytes: 0, new_bytes: 32 });
		},
		WriteOutcome::Overwritten(overwritten_bytes) => {
			gas!(
				context.interpreter,
				RuntimeCosts::SetStorage { old_bytes: overwritten_bytes, new_bytes: 32 }
			);
		},
		WriteOutcome::Taken(_) => {
			gas!(context.interpreter, RuntimeCosts::SetStorage { old_bytes: 32, new_bytes: 32 });
		},
	}
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);
	gas_legacy!(context.interpreter, gas::WARM_STORAGE_READ_COST);

	popn!([index, value], context.interpreter);

	let key = Key::Fix(index.to_be_bytes());
	let take_old = false;
	let _write_outcome = context.interpreter.extend.set_transient_storage(
		&key,
		Some(value.to_be_bytes::<32>().to_vec()),
		take_old,
	);

	// TODO: decide if we need to handle this outcome
	// Does it matter if the value was new or overwritten?
}

/// EIP-1153: Transient storage opcodes
/// Load value from transient storage
pub fn tload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, gas::WARM_STORAGE_READ_COST);

	popn_top!([], index, context.interpreter);

	let key = Key::Fix(index.to_be_bytes());
	let bytes = context.interpreter.extend.get_transient_storage(&key);
	*index = if let Some(storage_value) = bytes {
		if storage_value.len() != 32 {
			// tload always reads a word
			context.interpreter.halt(InstructionResult::Revert);
			return;
		}
		let mut bytes = [0u8; 32];
		bytes.copy_from_slice(&storage_value);
		U256::from_be_bytes(bytes)
	} else {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	};
}

/// Implements the LOG0-LOG4 instructions.
///
/// Appends log record with N topics.
pub fn log<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	gas_or_fail!(context.interpreter, gas::log_cost(N as u8, len as u64));
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
	popn!([beneficiary], context.interpreter);
	let h160 = sp_core::H160::from_slice(&beneficiary.to_be_bytes::<32>()[12..]);
	let dispatch_result = context.interpreter.extend.selfdestruct(&h160);

	match dispatch_result {
		Ok(_) => {
			context.interpreter.halt(InstructionResult::SelfDestruct);
			return;
		},
		Err(e) => {
			log::error!(target: LOG_TARGET, "Selfdestruct failed: {:?}", e);
			context.interpreter.halt(InstructionResult::Revert);
			return;
		},
	}
}
