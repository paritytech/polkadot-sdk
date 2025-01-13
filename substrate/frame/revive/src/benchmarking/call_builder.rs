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
	benchmarking::{default_deposit_limit, Contract, WasmModule},
	exec::{ExportedFunction, Ext, Key, Stack},
	storage::meter::Meter,
	transient_storage::MeterEntry,
	wasm::{PreparedCall, Runtime},
	BalanceOf, Config, DebugBuffer, Error, GasMeter, MomentOf, Origin, WasmBlob, Weight,
};
use alloc::{vec, vec::Vec};
use frame_benchmarking::benchmarking;
use sp_core::{H256, U256};

type StackExt<'a, T> = Stack<'a, T, WasmBlob<T>>;

/// A builder used to prepare a contract call.
pub struct CallSetup<T: Config> {
	contract: Contract<T>,
	dest: T::AccountId,
	origin: Origin<T>,
	gas_meter: GasMeter<T>,
	storage_meter: Meter<T>,
	value: BalanceOf<T>,
	debug_message: Option<DebugBuffer>,
	data: Vec<u8>,
	transient_storage_size: u32,
}

impl<T> Default for CallSetup<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn default() -> Self {
		Self::new(WasmModule::dummy())
	}
}

impl<T> CallSetup<T>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	/// Setup a new call for the given module.
	pub fn new(module: WasmModule) -> Self {
		let contract = Contract::<T>::new(module.clone(), vec![]).unwrap();
		let dest = contract.account_id.clone();
		let origin = Origin::from_account_id(contract.caller.clone());

		let storage_meter = Meter::new(&origin, default_deposit_limit::<T>(), 0u32.into()).unwrap();

		// Whitelist contract account, as it is already accounted for in the call benchmark
		benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&contract.account_id).into(),
		);

		// Whitelist the contract's contractInfo as it is already accounted for in the call
		// benchmark
		benchmarking::add_to_whitelist(
			crate::ContractInfoOf::<T>::hashed_key_for(&T::AddressMapper::to_address(
				&contract.account_id,
			))
			.into(),
		);

		Self {
			contract,
			dest,
			origin,
			gas_meter: GasMeter::new(Weight::MAX),
			storage_meter,
			value: 0u32.into(),
			debug_message: None,
			data: vec![],
			transient_storage_size: 0,
		}
	}

	/// Set the meter's storage deposit limit.
	pub fn set_storage_deposit_limit(&mut self, balance: BalanceOf<T>) {
		self.storage_meter = Meter::new(&self.origin, balance, 0u32.into()).unwrap();
	}

	/// Set the call's origin.
	pub fn set_origin(&mut self, origin: Origin<T>) {
		self.origin = origin;
	}

	/// Set the contract's balance.
	pub fn set_balance(&mut self, value: BalanceOf<T>) {
		self.contract.set_balance(value);
	}

	/// Set the call's input data.
	pub fn set_data(&mut self, value: Vec<u8>) {
		self.data = value;
	}

	/// Set the transient storage size.
	pub fn set_transient_storage_size(&mut self, size: u32) {
		self.transient_storage_size = size;
	}

	/// Set the debug message.
	pub fn enable_debug_message(&mut self) {
		self.debug_message = Some(Default::default());
	}

	/// Get the debug message.
	pub fn debug_message(&self) -> Option<DebugBuffer> {
		self.debug_message.clone()
	}

	/// Get the call's input data.
	pub fn data(&self) -> Vec<u8> {
		self.data.clone()
	}

	/// Get the call's contract.
	pub fn contract(&self) -> Contract<T> {
		self.contract.clone()
	}

	/// Build the call stack.
	pub fn ext(&mut self) -> (StackExt<'_, T>, WasmBlob<T>) {
		let mut ext = StackExt::bench_new_call(
			T::AddressMapper::to_address(&self.dest),
			self.origin.clone(),
			&mut self.gas_meter,
			&mut self.storage_meter,
			self.value,
			self.debug_message.as_mut(),
		);
		if self.transient_storage_size > 0 {
			Self::with_transient_storage(&mut ext.0, self.transient_storage_size).unwrap();
		}
		ext
	}

	/// Prepare a call to the module.
	pub fn prepare_call<'a>(
		ext: &'a mut StackExt<'a, T>,
		module: WasmBlob<T>,
		input: Vec<u8>,
	) -> PreparedCall<'a, StackExt<'a, T>> {
		module.prepare_call(Runtime::new(ext, input), ExportedFunction::Call).unwrap()
	}

	/// Add transient_storage
	fn with_transient_storage(ext: &mut StackExt<T>, size: u32) -> Result<(), &'static str> {
		let &MeterEntry { amount, limit } = ext.transient_storage().meter().current();
		ext.transient_storage().meter().current_mut().limit = size;
		for i in 1u32.. {
			let mut key_data = i.to_le_bytes().to_vec();
			while key_data.last() == Some(&0) {
				key_data.pop();
			}
			let key = Key::try_from_var(key_data).unwrap();
			if let Err(e) = ext.set_transient_storage(&key, Some(Vec::new()), false) {
				// Restore previous settings.
				ext.transient_storage().meter().current_mut().limit = limit;
				ext.transient_storage().meter().current_mut().amount = amount;
				if e == Error::<T>::OutOfTransientStorage.into() {
					break;
				} else {
					return Err("Initialization of the transient storage failed");
				}
			}
		}
		Ok(())
	}
}

#[macro_export]
macro_rules! memory(
	($($bytes:expr,)*) => {{
		vec![].iter()$(.chain($bytes.iter()))*.cloned().collect::<Vec<_>>()
	}};
);

#[macro_export]
macro_rules! build_runtime(
	($runtime:ident, $memory:ident: [$($segment:expr,)*]) => {
		$crate::build_runtime!($runtime, _contract, $memory: [$($segment,)*]);
	};
	($runtime:ident, $contract:ident, $memory:ident: [$($bytes:expr,)*]) => {
		$crate::build_runtime!($runtime, $contract);
		let mut $memory = $crate::memory!($($bytes,)*);
	};
	($runtime:ident, $contract:ident) => {
		let mut setup = CallSetup::<T>::default();
		let $contract = setup.contract();
		let input = setup.data();
		let (mut ext, _) = setup.ext();
		let mut $runtime = crate::wasm::Runtime::<_, [u8]>::new(&mut ext, input);
	};
);
