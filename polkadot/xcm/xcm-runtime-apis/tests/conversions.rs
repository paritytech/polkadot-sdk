// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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

mod mock;

use frame_support::{
	assert_err, assert_ok,
	sp_runtime::{
		testing::H256,
		traits::{IdentifyAccount, Verify},
		AccountId32, MultiSignature,
	},
};
use mock::*;
use sp_api::ProvideRuntimeApi;
use xcm::prelude::*;
use xcm_runtime_apis::conversions::{
	Error as LocationToAccountApiError, LocationToAccountApi, LocationToAccountHelper,
};

#[test]
fn convert_location_to_account_works() {
	sp_io::TestExternalities::default().execute_with(|| {
		let client = TestClient {};
		let runtime_api = client.runtime_api();

		// Test unknown conversion for `Here` location
		assert_err!(
			runtime_api
				.convert_location(H256::zero(), VersionedLocation::from(Location::here()))
				.unwrap(),
			LocationToAccountApiError::Unsupported
		);

		// Test known conversion for sibling parachain location
		assert_ok!(
			runtime_api
				.convert_location(H256::zero(), VersionedLocation::from((Parent, Parachain(1000))))
				.unwrap(),
			1000_u64
		);
	})
}

#[test]
fn location_to_account_helper_with_multi_signature_works() {
	type Signature = MultiSignature;
	type AccountIdForConversions = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
	// We alias only `Location::parent()`
	pub type LocationToAccountIdForConversions =
		(xcm_builder::ParentIsPreset<AccountIdForConversions>,);

	// Test unknown conversion for `Here` location
	assert_err!(
		LocationToAccountHelper::<
			AccountIdForConversions,
			LocationToAccountIdForConversions,
		>::convert_location(Location::here().into_versioned()),
		LocationToAccountApiError::Unsupported
	);

	// Test known conversion for `Parent` location
	assert_ok!(
		LocationToAccountHelper::<
			AccountIdForConversions,
			LocationToAccountIdForConversions,
		>::convert_location(Location::parent().into_versioned()),
		AccountId32::from(hex_literal::hex!("506172656e740000000000000000000000000000000000000000000000000000"))
	);
}
