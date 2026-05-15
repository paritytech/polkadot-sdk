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
	DispatchError, Error, Key, LOG_TARGET, RuntimeCosts, U256, limits,
	access_list::StorageAccessCost,
	metering::Token,
	storage::WriteOutcome,
	vec::Vec,
	vm::{
		Ext,
		evm::{
			Interpreter, instructions::utility::IntoAddress, interpreter::Halt,
			util::as_usize_or_halt,
		},
	},
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
	let key = Key::Fix(index.to_big_endian());
	// Pre-charge cold worst-case, then refund the cold/warm delta after the call.
	let charged = interpreter.ext.charge_or_halt(RuntimeCosts::GetStorage {
		len: 32,
		costs: StorageAccessCost::cold(),
	})?;
	let (value, costs) = interpreter.ext.get_storage(&key);
	interpreter
		.ext
		.frame_meter_mut()
		.adjust_weight(charged, RuntimeCosts::GetStorage { len: 32, costs });

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

/// Shared pre-charge + adjust scaffolding for SSTORE/TSTORE.
///
/// `C` carries any extra per-call state the cost token needs (e.g. the cold/warm
/// `StorageAccessCost` for persistent storage). `set_function` returns the
/// write outcome together with that state; `adjust_cost` builds the actual
/// cost token from the observed sizes and that state.
fn store_helper<'ext, E: Ext, C>(
	interpreter: &mut Interpreter<'ext, E>,
	cost_before: RuntimeCosts,
	set_function: fn(
		&mut E,
		&Key,
		Option<Vec<u8>>,
		bool,
	) -> (Result<WriteOutcome, DispatchError>, C),
	adjust_cost: fn(new_bytes: u32, old_bytes: u32, costs: C) -> RuntimeCosts,
) -> ControlFlow<Halt> {
	if interpreter.ext.is_read_only() {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}

	let [index, value] = interpreter.stack.popn()?;
	let charged_amount = interpreter.ext.charge_or_halt(cost_before)?;
	let key = Key::Fix(index.to_big_endian());
	let take_old = false;
	let value_to_store = if value.is_zero() { None } else { Some(value.to_big_endian().to_vec()) };
	let new_bytes = value_to_store.as_ref().map(|v| v.len() as u32).unwrap_or(0);
	let (result, costs) = set_function(interpreter.ext, &key, value_to_store, take_old);

	// Refund regardless of Ok/Err — the cold/warm signal is valid either way.
	// On Err the real `old_bytes` is unknown, so it stays at worst-case.
	let old_bytes = result.as_ref().map(|w| w.old_len()).unwrap_or(limits::STORAGE_BYTES);
	interpreter
		.ext
		.frame_meter_mut()
		.adjust_weight(charged_amount, adjust_cost(new_bytes, old_bytes, costs));

	match result {
		Ok(_) => ControlFlow::Continue(()),
		Err(_) => ControlFlow::Break(Error::<E::T>::ContractTrapped.into()),
	}
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let old_bytes = limits::STORAGE_BYTES;
	store_helper(
		interpreter,
		RuntimeCosts::SetStorage { new_bytes: 32, old_bytes, costs: StorageAccessCost::cold() },
		|ext, key, value, take_old| ext.set_storage(key, value, take_old),
		|new_bytes, old_bytes, costs| RuntimeCosts::SetStorage { new_bytes, old_bytes, costs },
	)
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let old_bytes = limits::STORAGE_BYTES;
	store_helper(
		interpreter,
		RuntimeCosts::SetTransientStorage { new_bytes: 32, old_bytes },
		|ext, key, value, take_old| (ext.set_transient_storage(key, value, take_old), ()),
		|new_bytes, old_bytes, ()| RuntimeCosts::SetTransientStorage { new_bytes, old_bytes },
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
	if len as u32 > limits::EVENT_BYTES {
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
pub fn selfdestruct<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	if interpreter.ext.is_read_only() {
		return ControlFlow::Break(Error::<E::T>::StateChangeDenied.into());
	}
	let [beneficiary] = interpreter.stack.popn()?;
	let charged = interpreter.ext.charge_or_halt(RuntimeCosts::Terminate { code_removed: true })?;
	let dispatch_result = interpreter.ext.terminate_if_same_tx(&beneficiary.into_address());

	match dispatch_result {
		Ok(code_removed) => {
			// halt execution on successful selfdestruct
			if matches!(code_removed, crate::CodeRemoved::No) {
				let actual_cost = RuntimeCosts::Terminate { code_removed: false };
				interpreter
					.ext
					.adjust_gas(charged, <RuntimeCosts as Token<E::T>>::weight(&actual_cost));
			}
			ControlFlow::Break(Halt::Return(Vec::default()))
		},
		Err(e) => {
			log::debug!(target: LOG_TARGET, "Selfdestruct failed: {:?}", e);
			ControlFlow::Break(Halt::Err(e))
		},
	}
}
