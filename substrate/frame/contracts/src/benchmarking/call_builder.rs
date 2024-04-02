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

/// A LeakReclaimer is a guard that reclaims the leaked box when dropped.
struct LeakReclaimer<T> {
	ptr: *mut T,
}

impl<T> LeakReclaimer<T> {
	/// Creates a new `LeakReclaimer`, leaking a boxed value and returning a static mutable
	/// reference to its content, along with the guard for later cleanup.
	///
	/// # Safety
	///
	/// The caller must ensure that the returned reference is not used after the `LeakReclaimer` is
	/// dropped.
	unsafe fn new(value: T) -> (&'static mut T, Self) {
		let mut boxed = Box::new(value);
		let ptr: *mut T = &mut *boxed;
		let static_ref: &'static mut T = Box::leak(boxed);

		(static_ref, Self { ptr })
	}
}

impl<T> Drop for LeakReclaimer<T> {
	fn drop(&mut self) {
		// Safety: The pointer is valid as it points to the leaked box, created by `[Self::new]`.
		unsafe {
			let _ = Box::from_raw(self.ptr);
		}
	}
}

/// A builder used to prepare a contract call.
pub struct CallBuilder<T: Config> {
	caller: T::AccountId,
	dest: T::AccountId,
	gas_meter: GasMeter<T>,
	storage_meter: Meter<T>,
	schedule: Schedule<T>,
	input: Vec<u8>,
}

/// A prepared contract call ready to be executed.
pub struct PreparedCall<'a, T: Config> {
	func: wasmi::Func,
	store: wasmi::Store<Runtime<'a, StackExt<'a, T>>>,

	// The reclaims are used to ensure that the leaked boxes are dropped when this get dropped.
	_reclaims: (LeakReclaimer<StackExt<'a, T>>, LeakReclaimer<CallBuilder<T>>),
}

impl<'a, T: Config> PreparedCall<'a, T> {
	pub fn call(mut self) {
		self.func.call(&mut self.store, &[], &mut []).unwrap();
	}
}

impl<T> CallBuilder<T>
where
	T: Config + pallet_balances::Config,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
{
	/// Create a new builder for the given module.
	pub fn new(module: WasmModule<T>) -> Self {
		let instance = Contract::<T>::new(module.clone(), vec![]).unwrap();
		let caller = instance.caller.clone();
		let dest = instance.account_id.clone();

		Self {
			caller,
			dest,
			schedule: T::Schedule::get(),
			gas_meter: GasMeter::new(Weight::MAX),
			storage_meter: Default::default(),
			input: vec![],
		}
	}

	/// Prepare a call to the module.
	/// Returns a a closure used to invoke the call.
	pub fn build(self) -> PreparedCall<'static, T> {
		let input = self.input.clone();

		// Safety: reclaim_sbox is dropped when PreparedCall is dropped.
		let (sbox, reclaim_sbox) = unsafe { LeakReclaimer::new(self) };

		let (ext, module) = Stack::bench_new_call(
			sbox.dest.clone(),
			Origin::from_account_id(sbox.caller.clone()),
			&mut sbox.gas_meter,
			&mut sbox.storage_meter,
			&sbox.schedule,
			0u32.into(),
			None,
			Determinism::Enforced,
		);

		// Safety: reclaim_ext is dropped when PreparedCall is dropped.
		let (ext, reclaim_ext): (&mut StackExt<T>, _) = unsafe { LeakReclaimer::new(ext) };
		let (func, store) = module.bench_prepare_call(ext, input);

		PreparedCall { func, store, _reclaims: (reclaim_ext, reclaim_sbox) }
	}
}
