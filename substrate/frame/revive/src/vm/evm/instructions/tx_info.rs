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

use crate::{address::AddressMapper, evm::runtime::GAS_PRICE, vm::RuntimeCosts};
use revm::primitives::{Address, U256};

use super::Context;
use crate::{vm::Ext, Config};

/// Implements the GASPRICE instruction.
///
/// Gets the gas price of the originating transaction.
pub fn gasprice<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::GasPrice);
	push!(context.interpreter, U256::from(GAS_PRICE));
}

/// Implements the ORIGIN instruction.
///
/// Gets the execution origination address.
pub fn origin<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Origin);
	match context.interpreter.extend.origin().account_id() {
		Ok(account_id) => {
			let address: Address = <E::T as Config>::AddressMapper::to_address(account_id).0.into();
			push!(context.interpreter, address.into_word().into());
		},
		Err(_) => {
			context
				.interpreter
				.halt(revm::interpreter::InstructionResult::FatalExternalError);
		},
	}
}

/// Implements the BLOBHASH instruction.
///
/// EIP-4844: Shard Blob Transactions - gets the hash of a transaction blob.
pub fn blob_hash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(revm::interpreter::InstructionResult::NotActivated);
}
