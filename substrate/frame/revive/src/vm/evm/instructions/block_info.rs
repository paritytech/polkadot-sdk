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
	vm::{
		evm::{interpreter::Halt, EVMGas, Interpreter, DIFFICULTY},
		Ext,
	},
	Error, RuntimeCosts,
};
use core::ops::ControlFlow;
use revm::interpreter::gas::BASE;
use sp_core::U256;

/// EIP-1344: ChainID opcode
pub fn chainid<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	interpreter.stack.push(interpreter.ext.chain_id())?;
	ControlFlow::Continue(())
}

/// Implements the COINBASE instruction.
///
/// Pushes the current block's beneficiary address onto the stack.
pub fn coinbase<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::BlockAuthor)?;
	let coinbase = interpreter.ext.block_author();
	interpreter.stack.push(coinbase)?;
	ControlFlow::Continue(())
}

/// Implements the TIMESTAMP instruction.
///
/// Pushes the current block's timestamp onto the stack.
pub fn timestamp<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::Now)?;
	let timestamp = interpreter.ext.now();
	interpreter.stack.push(timestamp)?;
	ControlFlow::Continue(())
}

/// Implements the NUMBER instruction.
///
/// Pushes the current block number onto the stack.
pub fn block_number<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::BlockNumber)?;
	let block_number = interpreter.ext.block_number();
	interpreter.stack.push(block_number)?;
	ControlFlow::Continue(())
}

/// Implements the DIFFICULTY/PREVRANDAO instruction.
///
/// Pushes the block difficulty (pre-merge) or prevrandao (post-merge) onto the stack.
pub fn difficulty<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	interpreter.stack.push(U256::from(DIFFICULTY))?;
	ControlFlow::Continue(())
}

/// Implements the GASLIMIT instruction.
///
/// Pushes the current block's gas limit onto the stack.
pub fn gaslimit<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::GasLimit)?;
	let gas_limit = interpreter.ext.gas_limit();
	interpreter.stack.push(U256::from(gas_limit))?;
	ControlFlow::Continue(())
}

/// EIP-3198: BASEFEE opcode
pub fn basefee<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::BaseFee)?;
	interpreter.stack.push(crate::Pallet::<E::T>::evm_base_fee())?;
	ControlFlow::Continue(())
}

/// EIP-7516: BLOBBASEFEE opcode is not supported
pub fn blob_basefee<'ext, E: Ext>(_interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	ControlFlow::Break(Error::<E::T>::InvalidInstruction.into())
}
