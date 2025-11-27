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

//! Benchmarks for Transaction Payment Pallet's transaction extension

extern crate alloc;

use super::*;
use frame_benchmarking::v2::*;
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use frame_system::{EventRecord, RawOrigin};
use sp_runtime::traits::{AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable};

/// Re-export the pallet for benchmarking with custom Config trait.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Benchmark configuration trait.
///
/// This extends the pallet's Config trait to allow runtimes to set up any
/// required state before running benchmarks. For example, runtimes that
/// distribute fees to block authors may need to set the author before
/// the benchmark runs.
pub trait Config: crate::Config {
	/// Called at the start of each benchmark to set up any required state.
	///
	/// The default implementation is a no-op. Runtimes can override this
	/// to perform setup like setting the block author for fee distribution.
	fn setup_benchmark_environment() {}
}

fn assert_last_event<T: crate::Config>(generic_event: <T as crate::Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks(where
	T: Config,
	T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn charge_transaction_payment() {
		T::setup_benchmark_environment();

		let caller: T::AccountId = account("caller", 0, 0);
		let existential_deposit =
			<T::OnChargeTransaction as OnChargeTransaction<T>>::minimum_balance();

		// Use a reasonable minimum tip that works for most runtimes
		let min_tip: BalanceOf<T> = 1_000_000_000u32.into();
		let tip = if existential_deposit.is_zero() { min_tip } else { existential_deposit };

		// Build the call and dispatch info first so we can compute the actual fee
		let ext: ChargeTransactionPayment<T> = ChargeTransactionPayment::from(tip);
		let inner = frame_system::Call::remark { remark: alloc::vec![] };
		let call = T::RuntimeCall::from(inner);
		let extension_weight = ext.weight(&call);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(100, 0),
			extension_weight,
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};

		// Calculate the actual fee that will be charged, then endow enough to cover it
		// with a 10x buffer to account for any fee multiplier variations.
		// Ensure we endow at least the existential deposit so the account can exist.
		let len: u32 = 10;
		let expected_fee = crate::Pallet::<T>::compute_fee(len, &info, tip);
		let amount_to_endow = expected_fee
			.saturating_mul(10u32.into())
			.max(existential_deposit);

		<T::OnChargeTransaction as OnChargeTransaction<T>>::endow_account(&caller, amount_to_endow);

		#[block]
		{
			assert!(ext
				.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, len as usize, 0, |_| Ok(
					post_info
				))
				.unwrap()
				.is_ok());
		}

		post_info.actual_weight.as_mut().map(|w| w.saturating_accrue(extension_weight));
		let actual_fee = crate::Pallet::<T>::compute_actual_fee(len, &info, &post_info, tip);
		assert_last_event::<T>(
			Event::<T>::TransactionFeePaid { who: caller, actual_fee, tip }.into(),
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
