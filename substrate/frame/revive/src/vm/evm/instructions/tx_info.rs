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

use revm::{
	interpreter::{
		gas as revm_gas,
		host::Host,
		interpreter_types::{RuntimeFlag, StackTr},
	},
	primitives::U256,
};

use super::Context;
use crate::vm::Ext;

/// Implements the GASPRICE instruction.
///
/// Gets the gas price of the originating transaction.
pub fn gasprice<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.host.effective_gas_price()));
}

/// Implements the ORIGIN instruction.
///
/// Gets the execution origination address.
pub fn origin<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.caller().into_word().into());
}

/// Implements the BLOBHASH instruction.
///
/// EIP-4844: Shard Blob Transactions - gets the hash of a transaction blob.
pub fn blob_hash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], index, context.interpreter);
	let i = as_usize_saturated!(index);
	*index = context.host.blob_hash(i).unwrap_or_default();
}
