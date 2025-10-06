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
	storage::WriteOutcome,
	vec::Vec,
	vm::{
		evm::{
			instructions::utility::IntoAddress, interpreter::Halt, util::as_usize_or_halt,
			Interpreter,
		},
		Ext,
	},
	DispatchError, Error, Key, RuntimeCosts, U256,
};
use core::ops::ControlFlow;

/// Implements the BALANCE instruction.
///
/// Gets the balance of the given account.
pub fn balance<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::BalanceOf)?;
	let ([], top) = interpreter.stack.popn_top()?;
	*top = interpreter.ext.balance_of(&top.into_address());
	ControlFlow::Continue(())
}

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn selfbalance<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::Balance)?;
	let balance = interpreter.ext.balance();
	interpreter.stack.push(balance)
}

/// Implements the EXTCODESIZE instruction.
///
/// Gets the size of an account's code.
pub fn extcodesize<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let ([], top) = interpreter.stack.popn_top()?;
	interpreter.ext.charge_or_halt(RuntimeCosts::CodeSize)?;
	let code_size = interpreter.ext.code_size(&top.into_address());
	*top = U256::from(code_size);
	ControlFlow::Continue(())
}

/// EIP-1052: EXTCODEHASH opcode
pub fn extcodehash<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let ([], top) = interpreter.stack.popn_top()?;
	interpreter.ext.charge_or_halt(RuntimeCosts::CodeHash)?;
	let code_hash = interpreter.ext.code_hash(&top.into_address());
	*top = U256::from_big_endian(&code_hash.0);
	ControlFlow::Continue(())
}

/// Implements the EXTCODECOPY instruction.
///
/// Copies a portion of an account's code to memory.
pub fn extcodecopy<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [address, memory_offset, code_offset, len] = interpreter.stack.popn()?;
	let len = as_usize_or_halt::<E::T>(len)?;
	interpreter.ext.charge_or_halt(RuntimeCosts::ExtCodeCopy(len as u32))?;
	if len == 0 {
		return ControlFlow::Continue(());
	}

	let address = address.into_address();
	let memory_offset = as_usize_or_halt::<E::T>(memory_offset)?;
	let code_offset = as_usize_or_halt::<E::T>(code_offset)?;

	interpreter.memory.resize(memory_offset, len)?;

	let mut buf = interpreter.memory.slice_mut(memory_offset, len);
	// Note: This can't panic because we resized memory to fit.
	interpreter.ext.copy_code_slice(&mut buf, &address, code_offset);
	ControlFlow::Continue(())
}

/// Implements the BLOCKHASH instruction.
///
/// Gets the hash of one of the 256 most recent complete blocks.
pub fn blockhash<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::BlockHash)?;
	let ([], number) = interpreter.stack.popn_top()?;

	// blockhash should push zero if number is not within valid range.
	if let Some(hash) = interpreter.ext.block_hash(*number) {
		*number = U256::from_big_endian(&hash.0)
	} else {
		*number = U256::zero()
	};
	ControlFlow::Continue(())
}

/// Implements the SLOAD instruction.
///
/// Loads a word from storage.
pub fn sload<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let ([], index) = interpreter.stack.popn_top()?;
	// NB: SLOAD loads 32 bytes from storage (i.e. U256).
	interpreter.ext.charge_or_halt(RuntimeCosts::GetStorage(32))?;
	let key = Key::Fix(index.to_big_endian());
	let value = interpreter.ext.get_storage(&key);

	*index = if let Some(storage_value) = value {
		// sload always reads a word
		let Ok::<[u8; 32], _>(bytes) = storage_value.try_into() else {
			log::debug!(target: crate::LOG_TARGET, "sload read invalid storage value length. Expected 32.");
			return ControlFlow::Break(Error::<E::T>::ContractTrapped.into());
		};
		U256::from_big_endian(&bytes)
	} else {
		// the key was never written before
		U256::zero()
	};
	ControlFlow::Continue(())
}

fn store_helper<'ext, E: Ext>(
	interpreter: &mut Interpreter<'ext, E>,
	cost_before: RuntimeCosts,
	set_function: fn(&mut E, &Key, Option<Vec<u8>>, bool) -> Result<WriteOutcome, DispatchError>,
	adjust_cost: fn(new_bytes: u32, old_bytes: u32) -> RuntimeCosts,
) -> ControlFlow<Halt> {
	if interpreter.ext.is_read_only() {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}

	let [index, value] = interpreter.stack.popn()?;

	// Charge gas before set_storage and later adjust it down to the true gas cost
	let charged_amount = interpreter.ext.charge_or_halt(cost_before)?;
	let key = Key::Fix(index.to_big_endian());
	let take_old = false;
	let Ok(write_outcome) =
		set_function(interpreter.ext, &key, Some(value.to_big_endian().to_vec()), take_old)
	else {
		return ControlFlow::Break(Error::<E::T>::ContractTrapped.into());
	};

	interpreter
		.ext
		.gas_meter_mut()
		.adjust_gas(charged_amount, adjust_cost(32, write_outcome.old_len()));

	ControlFlow::Continue(())
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let old_bytes = interpreter.ext.max_value_size();
	store_helper(
		interpreter,
		RuntimeCosts::SetStorage { new_bytes: 32, old_bytes },
		|ext, key, value, take_old| ext.set_storage(key, value, take_old),
		|new_bytes, old_bytes| RuntimeCosts::SetStorage { new_bytes, old_bytes },
	)
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let old_bytes = interpreter.ext.max_value_size();
	store_helper(
		interpreter,
		RuntimeCosts::SetTransientStorage { new_bytes: 32, old_bytes },
		|ext, key, value, take_old| ext.set_transient_storage(key, value, take_old),
		|new_bytes, old_bytes| RuntimeCosts::SetTransientStorage { new_bytes, old_bytes },
	)
}

/// EIP-1153: Transient storage opcodes
/// Load value from transient storage
pub fn tload<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let ([], index) = interpreter.stack.popn_top()?;
	interpreter.ext.charge_or_halt(RuntimeCosts::GetTransientStorage(32))?;

	let key = Key::Fix(index.to_big_endian());
	let bytes = interpreter.ext.get_transient_storage(&key);

	*index = if let Some(storage_value) = bytes {
		if storage_value.len() != 32 {
			// tload always reads a word
			log::debug!(target: crate::LOG_TARGET, "tload read invalid storage value length. Expected 32.");
			return ControlFlow::Break(Error::<E::T>::ContractTrapped.into());
		}

		let Ok::<[u8; 32], _>(bytes) = storage_value.try_into() else {
			return ControlFlow::Break(Error::<E::T>::ContractTrapped.into());
		};
		U256::from_big_endian(&bytes)
	} else {
		// the key was never written before
		U256::zero()
	};
	ControlFlow::Continue(())
}

/// Implements the LOG0-LOG4 instructions.
///
/// Appends log record with N topics.
pub fn log<'ext, const N: usize, E: Ext>(
	interpreter: &mut Interpreter<'ext, E>,
) -> ControlFlow<Halt> {
	if interpreter.ext.is_read_only() {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}

	let [offset, len] = interpreter.stack.popn()?;
	let len = as_usize_or_halt::<E::T>(len)?;
	if len as u32 > interpreter.ext.max_value_size() {
		return ControlFlow::Break(Error::<E::T>::OutOfGas.into());
	}

	let cost = RuntimeCosts::DepositEvent { num_topic: N as u32, len: len as u32 };
	interpreter.ext.charge_or_halt(cost)?;

	let data = if len == 0 {
		Vec::new()
	} else {
		let offset = as_usize_or_halt::<E::T>(offset)?;
		interpreter.memory.resize(offset, len)?;
		interpreter.memory.slice(offset..offset + len).to_vec()
	};
	if interpreter.stack.len() < N {
		return ControlFlow::Break(Error::<E::T>::StackUnderflow.into());
	}
	let topics = interpreter.stack.popn::<N>()?;
	let topics = topics.into_iter().map(|v| sp_core::H256::from(v.to_big_endian())).collect();

	interpreter.ext.deposit_event(topics, data.to_vec());
	ControlFlow::Continue(())
}

/// Implements the SELFDESTRUCT instruction.
///
/// Halt execution and register account for later deletion.
pub fn selfdestruct<'ext, E: Ext>(_interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	// TODO: for now this instruction is not supported
	ControlFlow::Break(Error::<E::T>::InvalidInstruction.into())
}
