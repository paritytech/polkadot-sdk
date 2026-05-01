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
	pallet_dummy::Call, DummyExtension, PrepareCount, Runtime, RuntimeCall, ValidateCount,
};
use frame_support::dispatch::DispatchInfo;
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
