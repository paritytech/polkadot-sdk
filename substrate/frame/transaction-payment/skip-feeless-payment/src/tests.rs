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

use super::*;
use crate::mock::{
	new_test_ext, pallet_dummy::Call, DummyExtension, PostDispatchCount, PrepareCount, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, System, ValidateCount, DUMMY_UNSPENT_WEIGHT,
};
use frame_support::dispatch::{DispatchInfo, Pays, PostDispatchInfo};
use sp_runtime::{traits::DispatchTransaction, transaction_validity::TransactionSource};

#[test]
fn skip_feeless_payment_works() {
	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 1 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_and_prepare(Some(0).into(), &call, &DispatchInfo::default(), 0, 0)
		.unwrap();
	assert_eq!(PrepareCount::get(), 1);

	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 0 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_and_prepare(Some(0).into(), &call, &DispatchInfo::default(), 0, 0)
		.unwrap();
	assert_eq!(PrepareCount::get(), 1);
}

#[test]
fn validate_works() {
	assert_eq!(ValidateCount::get(), 0);

	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 1 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_only(
			Some(0).into(),
			&call,
			&DispatchInfo::default(),
			0,
			TransactionSource::External,
			0,
		)
		.unwrap();
	assert_eq!(ValidateCount::get(), 1);
	assert_eq!(PrepareCount::get(), 0);

	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 0 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_only(
			Some(0).into(),
			&call,
			&DispatchInfo::default(),
			0,
			TransactionSource::External,
			0,
		)
		.unwrap();
	assert_eq!(ValidateCount::get(), 1);
	assert_eq!(PrepareCount::get(), 0);
}

#[test]
fn validate_prepare_works() {
	assert_eq!(ValidateCount::get(), 0);

	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 1 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_and_prepare(Some(0).into(), &call, &DispatchInfo::default(), 0, 0)
		.unwrap();
	assert_eq!(ValidateCount::get(), 1);
	assert_eq!(PrepareCount::get(), 1);

	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 0 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_and_prepare(Some(0).into(), &call, &DispatchInfo::default(), 0, 0)
		.unwrap();
	assert_eq!(ValidateCount::get(), 1);
	assert_eq!(PrepareCount::get(), 1);

	// Changes from previous prepare calls persist.
	let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 1 });
	SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
		.validate_and_prepare(Some(0).into(), &call, &DispatchInfo::default(), 0, 0)
		.unwrap();
	assert_eq!(ValidateCount::get(), 2);
	assert_eq!(PrepareCount::get(), 2);
}

#[test]
fn post_dispatch_feeless_emits_fee_skipped() {
	new_test_ext().execute_with(|| {
		let origin = RuntimeOrigin::signed(1);
		let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 0 });
		let (pre, _origin) = SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
			.validate_and_prepare(origin.clone(), &call, &DispatchInfo::default(), 0, 0)
			.unwrap();

		assert_eq!(
			SkipCheckIfFeeless::<Runtime, DummyExtension>::post_dispatch(
				pre,
				&DispatchInfo::default(),
				&mut PostDispatchInfo::default(),
				0,
				&Ok(()),
			),
			Ok(())
		);
		System::assert_last_event(
			RuntimeEvent::SkipFeeless(crate::Event::FeeSkipped { origin: origin.caller().clone() })
				.into(),
		);
		assert_eq!(PostDispatchCount::get(), 0);
	});
}

#[test]
fn post_dispatch_apply_forwards_to_inner() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 1 });
		let (pre, _origin) = SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
			.validate_and_prepare(Some(1).into(), &call, &DispatchInfo::default(), 0, 0)
			.unwrap();

		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(100, 0)),
			pays_fee: Pays::Yes,
		};
		assert_eq!(
			SkipCheckIfFeeless::<Runtime, DummyExtension>::post_dispatch(
				pre,
				&DispatchInfo::default(),
				&mut post_info,
				0,
				&Ok(()),
			),
			Ok(())
		);
		assert_eq!(PostDispatchCount::get(), 1);
		assert_eq!(
			post_info.actual_weight,
			Some(Weight::from_parts(100, 0).saturating_sub(DUMMY_UNSPENT_WEIGHT))
		);
		// No `FeeSkipped` event on the apply path.
		assert!(System::events().is_empty());
	});
}

#[test]
fn post_dispatch_feeless_returns_zero_weight() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::DummyPallet(Call::<Runtime>::aux { data: 0 });
		let (pre, _origin) = SkipCheckIfFeeless::<Runtime, DummyExtension>::from(DummyExtension)
			.validate_and_prepare(Some(1).into(), &call, &DispatchInfo::default(), 0, 0)
			.unwrap();

		assert_eq!(
			SkipCheckIfFeeless::<Runtime, DummyExtension>::post_dispatch_details(
				pre,
				&DispatchInfo::default(),
				&PostDispatchInfo::default(),
				0,
				&Ok(()),
			),
			Ok(Weight::zero())
		);
	});
}

#[test]
fn metadata_for_wrap_multiple_tx_ext() {
	let metadata = SkipCheckIfFeeless::<Runtime, (DummyExtension, DummyExtension)>::metadata();
	let mut expected_metadata = vec![];
	expected_metadata.extend(DummyExtension::metadata().into_iter());
	expected_metadata.extend(DummyExtension::metadata().into_iter());

	assert_eq!(metadata.len(), expected_metadata.len());
	for i in 0..expected_metadata.len() {
		assert_eq!(metadata[i].identifier, expected_metadata[i].identifier);
		assert_eq!(metadata[i].ty, expected_metadata[i].ty);
		assert_eq!(metadata[i].implicit, expected_metadata[i].implicit);
	}
}
