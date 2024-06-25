// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

mod mock;

use frame_support::{assert_err, sp_runtime::testing::H256};
use mock::*;
use sp_api::ProvideRuntimeApi;
use xcm::prelude::Location;
use xcm_runtime_api::conversions::{Error as LocationToAccountApiError, LocationToAccountApi};

#[test]
fn convert_location_to_account_works() {
	run_test(|| {
		let client = TestClient {};
		let runtime_api = client.runtime_api();

		// Test unknown conversion for `Here` location
		assert_err!(
			runtime_api
				.convert_location(H256::zero(), Location::here().into_versioned(), None)
				.unwrap(),
			LocationToAccountApiError::Unsupported
		);

		// Test known conversion for `Parent` location
		let result_ss58_default = runtime_api
			.convert_location(H256::zero(), Location::parent().into_versioned(), None)
			.unwrap()
			.expect("conversion works");
		let result_ss58_1 = runtime_api
			.convert_location(H256::zero(), Location::parent().into_versioned(), Some(1))
			.unwrap()
			.expect("conversion works");
		let result_ss58_2 = runtime_api
			.convert_location(H256::zero(), Location::parent().into_versioned(), Some(2))
			.unwrap()
			.expect("conversion works");
		let result_ss58_42 = runtime_api
			.convert_location(H256::zero(), Location::parent().into_versioned(), Some(42))
			.unwrap()
			.expect("conversion works");

		// `account_id` is the same
		assert_eq!(result_ss58_default.id, result_ss58_1.id);
		assert_eq!(result_ss58_1.id, result_ss58_2.id);
		assert_eq!(result_ss58_2.id, result_ss58_42.id);

		// but `address` changes according to the requested `ss58_version`
		assert_ne!(result_ss58_default.ss58.address, result_ss58_1.ss58.address);
		assert_ne!(result_ss58_default.ss58.address, result_ss58_2.ss58.address);
		assert_ne!(result_ss58_default.ss58.address, result_ss58_42.ss58.address);
		assert_ne!(result_ss58_1.ss58.address, result_ss58_2.ss58.address);
		assert_ne!(result_ss58_1.ss58.address, result_ss58_42.ss58.address);
		assert_ne!(result_ss58_2.ss58.address, result_ss58_42.ss58.address);

		assert_eq!(result_ss58_default.ss58.version, DefaultSs58Prefix::get());
		assert_eq!(result_ss58_1.ss58.version, 1);
		assert_eq!(result_ss58_2.ss58.version, 2);
		assert_eq!(result_ss58_42.ss58.version, 42);
	})
}
