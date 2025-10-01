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
	storage::WriteOutcome,
	vec::Vec,
	vm::{evm::U256Converter, Ext},
	DispatchError, Key, RuntimeCosts,
};
use revm::{
	interpreter::{interpreter_types::StackTr, InstructionResult},
	primitives::{Bytes, U256},
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
	let len = as_usize_or_fail!(context.interpreter, len_u256);

	gas!(context.interpreter, RuntimeCosts::ExtCodeCopy(len as u32));
	let address = sp_core::H160::from_slice(&address.to_be_bytes::<32>()[12..]);

	if len == 0 {
		return;
	}
	let memory_offset = as_usize_or_fail!(context.interpreter, memory_offset);
	let code_offset = as_usize_saturated!(code_offset);

	resize_memory!(context.interpreter, memory_offset, len);

	let mut buf = context.interpreter.memory.slice_mut(memory_offset, len);
	// Note: This can't panic because we resized memory to fit.
	context.interpreter.extend.copy_code_slice(&mut buf, &address, code_offset);
}

/// Implements the BLOCKHASH instruction.
///
/// Gets the hash of one of the 256 most recent complete blocks.
pub fn blockhash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockHash);
	popn_top!([], number, context.interpreter);
	let requested_number = <sp_core::U256 as U256Converter>::from_revm_u256(&number);

	// blockhash should push zero if number is not within valid range.
	if let Some(hash) = context.interpreter.extend.block_hash(requested_number) {
		*number = U256::from_be_bytes(hash.0)
	} else {
		*number = U256::ZERO
	};
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
		// sload always reads a word
		let Ok::<[u8; 32], _>(bytes) = storage_value.try_into() else {
			log::debug!(target: crate::LOG_TARGET, "sload read invalid storage value length. Expected 32.");
			context.interpreter.halt(InstructionResult::FatalExternalError);
			return
		};
		U256::from_be_bytes(bytes)
	} else {
		// the key was never written before
		U256::ZERO
	};
}

fn store_helper<'ext, E: Ext>(
	context: Context<'_, 'ext, E>,
	cost_before: RuntimeCosts,
	set_function: fn(&mut E, &Key, Option<Vec<u8>>, bool) -> Result<WriteOutcome, DispatchError>,
	adjust_cost: fn(new_bytes: u32, old_bytes: u32) -> RuntimeCosts,
) {
	if context.interpreter.extend.is_read_only() {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	}

	popn!([index, value], context.interpreter);

	// Charge gas before set_storage and later adjust it down to the true gas cost
	let Ok(charged_amount) = context.interpreter.extend.gas_meter_mut().charge(cost_before) else {
		context.interpreter.halt(InstructionResult::OutOfGas);
		return;
	};

	let key = Key::Fix(index.to_be_bytes());
	let take_old = false;
	let Ok(write_outcome) = set_function(
		context.interpreter.extend,
		&key,
		Some(value.to_be_bytes::<32>().to_vec()),
		take_old,
	) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};

	context
		.interpreter
		.extend
		.gas_meter_mut()
		.adjust_gas(charged_amount, adjust_cost(32, write_outcome.old_len()));
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	let old_bytes = context.interpreter.extend.max_value_size();
	store_helper(
		context,
		RuntimeCosts::SetStorage { new_bytes: 32, old_bytes },
		|ext, key, value, take_old| ext.set_storage(key, value, take_old),
		|new_bytes, old_bytes| RuntimeCosts::SetStorage { new_bytes, old_bytes },
	);
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	let old_bytes = context.interpreter.extend.max_value_size();
	store_helper(
		context,
		RuntimeCosts::SetTransientStorage { new_bytes: 32, old_bytes },
		|ext, key, value, take_old| ext.set_transient_storage(key, value, take_old),
		|new_bytes, old_bytes| RuntimeCosts::SetTransientStorage { new_bytes, old_bytes },
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
			log::error!(target: crate::LOG_TARGET, "tload read invalid storage value length. Expected 32.");
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
	if context.interpreter.extend.is_read_only() {
		context.interpreter.halt(InstructionResult::Revert);
		return;
	}

	popn!([offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	if len as u32 > context.interpreter.extend.max_value_size() {
		context
			.interpreter
			.halt(revm::interpreter::InstructionResult::InvalidOperandOOG);
		return;
	}

	gas!(context.interpreter, RuntimeCosts::DepositEvent { num_topic: N as u32, len: len as u32 });
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

	let topics = topics.into_iter().map(|v| sp_core::H256::from(v.to_be_bytes())).collect();

	context.interpreter.extend.deposit_event(topics, data.to_vec());
}

/// Implements the SELFDESTRUCT instruction.
///
/// Halt execution and register account for later deletion.
pub fn selfdestruct<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	// TODO: for now this instruction is not supported
	context.interpreter.halt(InstructionResult::NotActivated);
}
