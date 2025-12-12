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
	address::AddressMapper,
	vm::{
		evm::{interpreter::Halt, Interpreter},
		Ext, RuntimeCosts,
	},
	Config, Error,
};
use core::ops::ControlFlow;

/// Implements the GASPRICE instruction.
///
/// Gets the gas price of the originating transaction.
pub fn gasprice<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::GasPrice)?;
	interpreter.stack.push(interpreter.ext.effective_gas_price())
}

/// Implements the ORIGIN instruction.
///
/// Gets the execution origination address.
pub fn origin<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::Origin)?;
	match interpreter.ext.origin().account_id() {
		Ok(account_id) => {
			let address = <E::T as Config>::AddressMapper::to_address(account_id);
			interpreter.stack.push(address)
		},
		Err(_) => ControlFlow::Break(Error::<E::T>::ContractTrapped.into()),
	}
}

/// Implements the BLOBHASH instruction.
///
/// EIP-4844: Shard Blob Transactions - gets the hash of a transaction blob.
pub fn blob_hash<'ext, E: Ext>(_interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	ControlFlow::Break(Error::<E::T>::InvalidInstruction.into())
}
