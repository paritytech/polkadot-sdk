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
	vm::{
		evm::{U256Converter, BASE_FEE, DIFFICULTY},
		Ext,
	},
	RuntimeCosts,
};
use revm::{
	interpreter::gas as revm_gas,
	primitives::{Address, U256},
};
use sp_core::H160;

/// EIP-1344: ChainID opcode
pub fn chainid<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.extend.chain_id()));
}

/// Implements the COINBASE instruction.
///
/// Pushes the current block's beneficiary address onto the stack.
pub fn coinbase<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockAuthor);
	let coinbase: Address =
		context.interpreter.extend.block_author().unwrap_or(H160::zero()).0.into();
	push!(context.interpreter, coinbase.into_word().into());
}

/// Implements the TIMESTAMP instruction.
///
/// Pushes the current block's timestamp onto the stack.
pub fn timestamp<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Now);
	let timestamp = context.interpreter.extend.now();
	push!(context.interpreter, timestamp.into_revm_u256());
}

/// Implements the NUMBER instruction.
///
/// Pushes the current block number onto the stack.
pub fn block_number<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockNumber);
	let block_number = context.interpreter.extend.block_number();
	push!(context.interpreter, block_number.into_revm_u256());
}

/// Implements the DIFFICULTY/PREVRANDAO instruction.
///
/// Pushes the block difficulty (pre-merge) or prevrandao (post-merge) onto the stack.
pub fn difficulty<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(DIFFICULTY));
}

/// Implements the GASLIMIT instruction.
///
/// Pushes the current block's gas limit onto the stack.
pub fn gaslimit<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::GasLimit);
	let gas_limit = context.interpreter.extend.gas_limit();
	push!(context.interpreter, U256::from(gas_limit));
}

/// EIP-3198: BASEFEE opcode
pub fn basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BaseFee);
	push!(context.interpreter, BASE_FEE.into_revm_u256());
}

/// EIP-7516: BLOBBASEFEE opcode is not supported
pub fn blob_basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(revm::interpreter::InstructionResult::NotActivated);
}
