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
	BalanceOf, Config, Determinism, GasMeter, Origin, Schedule, TypeInfo, WasmBlob, Weight,
};
use codec::{Encode, HasCompact};
use core::fmt::Debug;
use sp_core::Get;
use sp_std::prelude::*;

type StackExt<'a, T> = Stack<'a, T, WasmBlob<T>>;

/// A builder used to prepare a contract call.
pub struct BenchCall<T: Config> {
	contract: Contract<T>,
	gas_meter: GasMeter<T>,
	storage_meter: Meter<T>,
	schedule: Schedule<T>,
	input: Vec<u8>,
}

/// A prepared contract call ready to be executed.
pub struct PreparedCall<'a, T: Config> {
	func: wasmi::Func,
	store: wasmi::Store<Runtime<'a, StackExt<'a, T>>>,
}

impl<'a, T: Config> PreparedCall<'a, T> {
	pub fn call(mut self) {
		self.func.call(&mut self.store, &[], &mut []).unwrap();
	}
}

impl<T> BenchCall<T>
where
	T: Config + pallet_balances::Config,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
{
	/// Create a new builder for the given module.
	pub fn new(module: WasmModule<T>) -> Self {
		let contract = Contract::<T>::new(module.clone(), vec![]).unwrap();

		Self {
			contract,
			schedule: T::Schedule::get(),
			gas_meter: GasMeter::new(Weight::MAX),
			storage_meter: Default::default(),
			input: vec![],
		}
	}

	pub fn input(&self) -> Vec<u8> {
		self.input.clone()
	}

	pub fn contract(&self) -> Contract<T> {
		self.contract.clone()
	}

	/// Build the call stack.
	pub fn ext(&mut self) -> (StackExt<'_, T>, WasmBlob<T>) {
		let caller = self.contract.caller.clone();
		let dest = self.contract.account_id.clone();

		StackExt::bench_new_call(
			dest,
			Origin::from_account_id(caller),
			&mut self.gas_meter,
			&mut self.storage_meter,
			&self.schedule,
			0u32.into(),
			None,
			Determinism::Enforced,
		)
	}

	/// Prepare a call to the module.
	/// Returns a a closure used to invoke the call.
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
macro_rules! call_builder(
	($func: ident, $contract: ident, $module:expr) => {
		let mut bench_call = BenchCall::<T>::new($module);
		let input = bench_call.input();
		let $contract = bench_call.contract();
		let (mut ext, module) = bench_call.ext();
		let $func = BenchCall::<T>::prepare_call(&mut ext, module, input);
	};
	($func: ident, $module:expr) => {
		$crate::call_builder!($func, _info, $module);
	}
);
