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

use super::*;
use crate::Pallet;
use frame_benchmarking::v2::*;
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use frame_system::{EventRecord, RawOrigin};
use sp_runtime::traits::{DispatchTransaction, Dispatchable};

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks(where
	T: Config,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
    BalanceOf<T>: Send + Sync + From<u64>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn charge_transaction_payment() {
		let caller: T::AccountId = whitelisted_caller();
		<T::OnChargeTransaction as OnChargeTransaction<T>>::endow_account(
			&caller,
			<T::OnChargeTransaction as OnChargeTransaction<T>>::minimum_balance() * 1000.into(),
		);
		let tip = <T::OnChargeTransaction as OnChargeTransaction<T>>::minimum_balance();
		let ext: ChargeTransactionPayment<T> = ChargeTransactionPayment::from(tip);
		let inner = frame_system::Call::remark { remark: vec![] };
		let call = T::RuntimeCall::from(inner);
		let info = DispatchInfo {
			weight: Weight::from_parts(100, 0),
			class: DispatchClass::Operational,
			pays_fee: Pays::Yes,
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(10, 0)),
			pays_fee: Pays::Yes,
		};

		#[block]
		{
			assert!(ext
				.test_run(RawOrigin::Signed(caller.clone()).into(), &call, &info, 10, |_| Ok(
					post_info
				))
				.unwrap()
				.is_ok());
		}

		let actual_fee = Pallet::<T>::compute_actual_fee(10, &info, &post_info, tip);
		assert_last_event::<T>(
			Event::<T>::TransactionFeePaid { who: caller, actual_fee, tip }.into(),
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
