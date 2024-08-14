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
	benchmarking::{Contract, WasmModule},
	exec::Stack,
	storage::meter::Meter,
	wasm::Runtime,
	BalanceOf, Config, DebugBufferVec, Determinism, ExecReturnValue, GasMeter, Origin, Schedule,
	TypeInfo, WasmBlob, Weight,
};
use codec::{Encode, HasCompact};
use core::fmt::Debug;
use frame_benchmarking::benchmarking;
use sp_core::Get;
use sp_std::prelude::*;

type StackExt<'a, T> = Stack<'a, T, WasmBlob<T>>;

/// A prepared contract call ready to be executed.
pub struct PreparedCall<'a, T: Config> {
	func: wasmi::Func,
	store: wasmi::Store<Runtime<'a, StackExt<'a, T>>>,
}

impl<'a, T: Config> PreparedCall<'a, T> {
	pub fn call(mut self) -> ExecReturnValue {
		let result = self.func.call(&mut self.store, &[], &mut []);
		WasmBlob::<T>::process_result(self.store, result).unwrap()
	}
}

/// A builder used to prepare a contract call.
pub struct CallSetup<T: Config> {
	contract: Contract<T>,
	dest: T::AccountId,
	origin: Origin<T>,
	gas_meter: GasMeter<T>,
	storage_meter: Meter<T>,
	schedule: Schedule<T>,
	value: BalanceOf<T>,
	debug_message: Option<DebugBufferVec<T>>,
	determinism: Determinism,
	data: Vec<u8>,
}

impl<T> Default for CallSetup<T>
where
	T: Config + pallet_balances::Config,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
{
	fn default() -> Self {
		Self::new(WasmModule::dummy())
	}
}

impl<T> CallSetup<T>
where
	T: Config + pallet_balances::Config,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
{
	/// Setup a new call for the given module.
	pub fn new(module: WasmModule<T>) -> Self {
		let contract = Contract::<T>::new(module.clone(), vec![]).unwrap();
		let dest = contract.account_id.clone();
		let origin = Origin::from_account_id(contract.caller.clone());

		let storage_meter = Meter::new(&origin, None, 0u32.into()).unwrap();

		// Whitelist contract account, as it is already accounted for in the call benchmark
		benchmarking::add_to_whitelist(
			frame_system::Account::<T>::hashed_key_for(&contract.account_id).into(),
		);

		// Whitelist the contract's contractInfo as it is already accounted for in the call
		// benchmark
		benchmarking::add_to_whitelist(
			crate::ContractInfoOf::<T>::hashed_key_for(&contract.account_id).into(),
		);

		Self {
			contract,
			dest,
			origin,
			gas_meter: GasMeter::new(Weight::MAX),
			storage_meter,
			schedule: T::Schedule::get(),
			value: 0u32.into(),
			debug_message: None,
			determinism: Determinism::Enforced,
			data: vec![],
		}
	}

	/// Set the meter's storage deposit limit.
	pub fn set_storage_deposit_limit(&mut self, balance: BalanceOf<T>) {
		self.storage_meter = Meter::new(&self.origin, Some(balance), 0u32.into()).unwrap();
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

	/// Set the debug message.
	pub fn enable_debug_message(&mut self) {
		self.debug_message = Some(Default::default());
	}

	/// Get the debug message.
	pub fn debug_message(&self) -> Option<DebugBufferVec<T>> {
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
		StackExt::bench_new_call(
			self.dest.clone(),
			self.origin.clone(),
			&mut self.gas_meter,
			&mut self.storage_meter,
			&self.schedule,
			self.value,
			self.debug_message.as_mut(),
			self.determinism,
		)
	}

	/// Prepare a call to the module.
	pub fn prepare_call<'a>(
		ext: &'a mut StackExt<'a, T>,
		module: WasmBlob<T>,
		input: Vec<u8>,
	) -> PreparedCall<'a, T> {
		let (func, store) = module.bench_prepare_call(ext, input);
		PreparedCall { func, store }
	}
}

#[macro_export]
macro_rules! memory(
	($($bytes:expr,)*) => {
		 vec![]
		    .into_iter()
		    $(.chain($bytes))*
		    .collect::<Vec<_>>()
	};
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
		let mut $runtime = crate::wasm::Runtime::new(&mut ext, input);
	};
);
