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
use crate::{vm::Ext, RuntimeCosts};
use revm::{
	interpreter::{gas as revm_gas, host::Host, interpreter_types::RuntimeFlag},
	primitives::{hardfork::SpecId::*, U256},
};

/// EIP-1344: ChainID opcode
pub fn chainid<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, ISTANBUL);
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.chain_id());
}

/// Implements the COINBASE instruction.
///
/// Pushes the current block's beneficiary address onto the stack.
pub fn coinbase<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.beneficiary().into_word().into());
}

/// Implements the TIMESTAMP instruction.
///
/// Pushes the current block's timestamp onto the stack.
pub fn timestamp<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.timestamp());
}

/// Implements the NUMBER instruction.
///
/// Pushes the current block number onto the stack.
pub fn block_number<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::BlockNumber);
	let block_number = context.interpreter.extend.block_number();
	push!(context.interpreter, U256::from_limbs(block_number.0));
}

/// Implements the DIFFICULTY/PREVRANDAO instruction.
///
/// Pushes the block difficulty (pre-merge) or prevrandao (post-merge) onto the stack.
pub fn difficulty<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	if context.interpreter.runtime_flag.spec_id().is_enabled_in(MERGE) {
		// Unwrap is safe as this fields is checked in validation handler.
		push!(context.interpreter, context.host.prevrandao().unwrap());
	} else {
		push!(context.interpreter, context.host.difficulty());
	}
}

/// Implements the GASLIMIT instruction.
///
/// Pushes the current block's gas limit onto the stack.
pub fn gaslimit<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.gas_limit());
}

/// EIP-3198: BASEFEE opcode
pub fn basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, LONDON);
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.basefee());
}

/// EIP-7516: BLOBBASEFEE opcode
pub fn blob_basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.blob_gasprice());
}
