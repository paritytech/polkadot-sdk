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
	assert_ok,
	tests::{
		frame_system::{Numbers, Total},
		new_test_ext, Runtime, RuntimeOrigin, RuntimeTask, System,
	},
};
use frame_support_procedural::pallet_section;

#[pallet_section]
mod tasks_example {
	#[docify::export(tasks_example)]
	#[pallet::tasks_experimental]
	impl<T: Config> Pallet<T> {
		/// Add a pair of numbers into the totals and remove them.
		#[pallet::task_list(Numbers::<T>::iter_keys())]
		#[pallet::task_condition(|i| Numbers::<T>::contains_key(i))]
		#[pallet::task_weight(0.into())]
		#[pallet::task_index(0)]
		pub fn add_number_into_total(i: u32) -> DispatchResult {
			let v = Numbers::<T>::take(i).ok_or(Error::<T>::NotFound)?;
			Total::<T>::mutate(|(total_keys, total_values)| {
				*total_keys += i;
				*total_values += v;
			});
			Ok(())
		}
	}
}

#[docify::export]
#[test]
fn tasks_work() {
	new_test_ext().execute_with(|| {
		Numbers::<Runtime>::insert(0, 1);

		let task = RuntimeTask::System(super::frame_system::Task::<Runtime>::AddNumberIntoTotal {
			i: 0u32,
		});

		assert_ok!(System::do_task(RuntimeOrigin::signed(1), task.clone(),));
		assert_eq!(Numbers::<Runtime>::get(0), None);
		assert_eq!(Total::<Runtime>::get(), (0, 1));
	});
}
