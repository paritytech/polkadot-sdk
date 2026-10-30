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
	pallet::{self, Pallet},
	pallet2,
};
use codec::{Decode, Encode};
use scale_info::meta_type;

use frame_support::{derive_impl, pallet_prelude::PalletInfoAccess, view_functions::ViewFunction};
use sp_io::hashing::twox_128;
use sp_metadata_ir::{
	ItemDeprecationInfoIR, PalletViewFunctionMetadataIR, PalletViewFunctionParamMetadataIR,
};
use sp_runtime::testing::TestXt;

pub type AccountId = u32;
pub type Balance = u32;

type Block = frame_system::mocking::MockBlock<Runtime>;
frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		ViewFunctionsExample: pallet,
		ViewFunctionsInstance: pallet2,
		ViewFunctionsInstance1: pallet2::<Instance1>,
	}
);

pub type Extrinsic = TestXt<RuntimeCall, ()>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl pallet::Config for Runtime {}
impl pallet2::Config<pallet2::Instance1> for Runtime {}

impl pallet2::Config for Runtime {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;

	let t = RuntimeGenesisConfig { system: Default::default() }.build_storage().unwrap();
	t.into()
}

#[test]
fn pallet_get_value_query() {
	new_test_ext().execute_with(|| {
		let some_value = Some(99);
		pallet::SomeValue::<Runtime>::set(some_value);
		assert_eq!(some_value, Pallet::<Runtime>::get_value());

		let query = pallet::GetValueViewFunction::<Runtime>::new();
		test_dispatch_view_function(&query, some_value);
	});
}

#[test]
fn pallet_get_value_with_arg_query() {
	new_test_ext().execute_with(|| {
		let some_key = 1u32;
		let some_value = Some(123);
		pallet::SomeMap::<Runtime>::set(some_key, some_value);
		assert_eq!(some_value, Pallet::<Runtime>::get_value_with_arg(some_key));

		let query = pallet::GetValueWithArgViewFunction::<Runtime>::new(some_key);
		test_dispatch_view_function(&query, some_value);
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

		let query = pallet2::GetValueViewFunction::<Runtime>::new();
		test_dispatch_view_function(&query, instance_value);

		let query_instance1 = pallet2::GetValueViewFunction::<Runtime, Instance1>::new();
		test_dispatch_view_function(&query_instance1, instance1_value);
	});
}

#[test]
fn metadata_ir_definitions() {
	new_test_ext().execute_with(|| {
		let metadata_ir = Runtime::metadata_ir();
		let pallet1 = metadata_ir
			.pallets
			.iter()
			.find(|pallet| pallet.name == "ViewFunctionsExample")
			.unwrap();

		fn view_fn_id(preifx_hash: [u8; 16], view_fn_signature: &str) -> [u8; 32] {
			let mut id = [0u8; 32];
			id[..16].copy_from_slice(&preifx_hash);
			id[16..].copy_from_slice(&twox_128(view_fn_signature.as_bytes()));
			id
		}

		let get_value_id = view_fn_id(
			<ViewFunctionsExample as PalletInfoAccess>::name_hash(),
			"get_value() -> Option<u32>",
		);

		let get_value_with_arg_id = view_fn_id(
			<ViewFunctionsExample as PalletInfoAccess>::name_hash(),
			"get_value_with_arg(u32) -> Option<u32>",
		);

		pretty_assertions::assert_eq!(
			pallet1.view_functions,
			vec![
				PalletViewFunctionMetadataIR {
					name: "get_value",
					id: get_value_id,
					inputs: vec![],
					output: meta_type::<Option<u32>>(),
					docs: vec![" Query value with no input args."],
					deprecation_info: ItemDeprecationInfoIR::NotDeprecated,
				},
				PalletViewFunctionMetadataIR {
					name: "get_value_with_arg",
					id: get_value_with_arg_id,
					inputs: vec![PalletViewFunctionParamMetadataIR {
						name: "key",
						ty: meta_type::<u32>()
					},],
					output: meta_type::<Option<u32>>(),
					docs: vec![" Query value with input args."],
					deprecation_info: ItemDeprecationInfoIR::NotDeprecated,
				},
			]
		);
	});
}

fn test_dispatch_view_function<Q, V>(query: &Q, expected: V)
where
	Q: ViewFunction + Encode,
	V: Decode + Eq + PartialEq + std::fmt::Debug,
{
	let input = query.encode();
	let output = Runtime::execute_view_function(Q::id(), input).unwrap();
	let query_result = V::decode(&mut &output[..]).unwrap();

	assert_eq!(expected, query_result,);
}
