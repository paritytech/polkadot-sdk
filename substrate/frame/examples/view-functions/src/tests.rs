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

//! Tests for `pallet-example-tasks`.
#![cfg(test)]

use crate::{
	mock::*,
	pallet::{self, Pallet},
};
use codec::{Decode, Encode};
use frame_support::traits::{DispatchQuery, Query};

#[test]
fn pallet_get_value_query() {
	new_test_ext().execute_with(|| {
		let some_value = Some(99);
		pallet::SomeValue::<Runtime>::set(some_value);
		assert_eq!(some_value, Pallet::<Runtime>::get_value());

		let query = pallet::GetValueQuery::<Runtime>::new();
		let input = query.encode();
		let mut output = Vec::new();

		let _ = <Pallet<Runtime> as DispatchQuery>::dispatch_query::<_, Vec<u8>>(
			&<pallet::GetValueQuery<Runtime> as Query>::ID,
			&mut &input[..],
			&mut output,
		)
		.unwrap();

		let query_result = <Option<u32>>::decode(&mut &output[..]).unwrap();

		assert_eq!(some_value, query_result,);
	});
}
