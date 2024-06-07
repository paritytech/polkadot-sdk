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

//! Tests for `pallet-example-view-functions`.
#![cfg(test)]

use crate::{
	mock::*,
	pallet::{self, Pallet},
	pallet2,
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
		test_dispatch_query(&query, some_value);
	});
}

#[test]
fn pallet_get_value_with_arg_query() {
	new_test_ext().execute_with(|| {
		let some_key = 1u32;
		let some_value = Some(123);
		pallet::SomeMap::<Runtime>::set(some_key, some_value);
		assert_eq!(some_value, Pallet::<Runtime>::get_value_with_arg(some_key));

		let query = pallet::GetValueWithArgQuery::<Runtime>::new(some_key);
		test_dispatch_query(&query, some_value);
	});
}

#[test]
fn pallet_multiple_instances() {
	use pallet2::Instance1;

	new_test_ext().execute_with(|| {
		let instance_value = Some(123);
		let instance1_value = Some(456);

		pallet2::SomeValue::<Runtime>::set(instance_value);
		pallet2::SomeValue::<Runtime, Instance1>::set(instance1_value);

		let query = pallet2::GetValueQuery::<Runtime>::new();
		test_dispatch_query(&query, instance_value);

		let query_instance1 = pallet2::GetValueQuery::<Runtime, Instance1>::new();
		test_dispatch_query(&query_instance1, instance1_value);
	});
}

fn test_dispatch_query<Q, V>(query: &Q, expected: V)
where
	Q: Query + Encode,
	V: Decode + Eq + PartialEq + std::fmt::Debug,
{
	let input = query.encode();
	let output = Runtime::execute_query(Q::id(), input).unwrap();
	let query_result = V::decode(&mut &output[..]).unwrap();

	assert_eq!(expected, query_result,);
}
