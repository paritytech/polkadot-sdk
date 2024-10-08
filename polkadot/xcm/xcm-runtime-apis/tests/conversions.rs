// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

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
