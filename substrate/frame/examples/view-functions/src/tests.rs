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

		let _ = <Pallet<Runtime> as DispatchQuery>::dispatch_query::<Vec<u8>>(
			&<pallet::GetValueQuery<Runtime> as Query>::ID,
			&mut &input[..],
			&mut output,
		)
		.unwrap();

		let query_result = <Option<u32>>::decode(&mut &output[..]).unwrap();

		assert_eq!(some_value, query_result,);
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
		let input = query.encode();
		let mut output = Vec::new();

		let _ = <Pallet<Runtime> as DispatchQuery>::dispatch_query::<Vec<u8>>(
			&<pallet::GetValueWithArgQuery<Runtime> as Query>::ID,
			&mut &input[..],
			&mut output,
		)
		.unwrap();

		let query_result = <Option<u32>>::decode(&mut &output[..]).unwrap();

		assert_eq!(some_value, query_result,);
	});
}

// pub struct Test<T>(PhantomData<T>);
//
// impl<T: pallet::Config> DispatchQuery for Test<T>
// where
// 	T::AccountId: From<crate::SomeType1> + crate::SomeAssociation1,
// {
// 	#[automatically_derived]
// 	// #[deny(unreachable_patterns)]
// 	fn dispatch_query<O: codec::Output>(
// 		id: &frame_support::traits::QueryId,
// 		input: &mut &[u8],
// 		output: &mut O,
// 	) -> Result<(), codec::Error> {
// 		match id.suffix {
// 			<pallet::GetValueQuery<T> as frame_support::traits::QueryIdSuffix>::SUFFIX => {
// 				let query = <pallet::GetValueQuery<
// 					T,
// 				> as codec::DecodeAll>::decode_all(input)?;
// 				let result = <pallet::GetValueQuery<
// 					T,
// 				> as frame_support::traits::Query>::query(query);
// 				let output = codec::Encode::encode_to(
// 					&result,
// 					output,
// 				);
// 				::core::result::Result::Ok(output)
// 			}
// 			<pallet::GetValueWithArgQuery<
// 				T,
// 			> as frame_support::traits::QueryIdSuffix>::SUFFIX => {
// 				let query = <pallet::GetValueWithArgQuery<
// 					T,
// 				> as codec::DecodeAll>::decode_all(input)?;
// 				let result = <pallet::GetValueWithArgQuery<
// 					T,
// 				> as frame_support::traits::Query>::query(query);
// 				let output = codec::Encode::encode_to(
// 					&result,
// 					output,
// 				);
// 				::core::result::Result::Ok(output)
// 			}
// 			_ => {
// 				Err(
// 					codec::Error::from(
// 						"DispatchQuery not implemented",
// 					),
// 				)
// 			}
// 		}
// 	}
//
// }
