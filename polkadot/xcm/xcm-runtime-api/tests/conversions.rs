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
pub fn convert_location_to_account_works() {
	run_test(|| {
		let client = TestClient {};
		let runtime_api = client.runtime_api();

		// Test unknown conversion for `Here` location
		assert_err!(
			runtime_api.convert(H256::zero(), Location::here(), None).unwrap(),
			LocationToAccountApiError::Unsupported
		);

		// Test known conversion for `Parent` location
		let result_ss58_default = runtime_api
			.convert(H256::zero(), Location::parent(), None)
			.unwrap()
			.expect("conversion works");
		let result_ss58_1 = runtime_api
			.convert(H256::zero(), Location::parent(), Some(1))
			.unwrap()
			.expect("conversion works");

		assert_eq!(result_ss58_default.id, result_ss58_1.id);
		assert_ne!(result_ss58_default.ss58.address, result_ss58_1.ss58.address);
		assert_ne!(result_ss58_default.ss58.version, result_ss58_1.ss58.version);
		assert_eq!(
			(result_ss58_default.ss58.version, result_ss58_1.ss58.version),
			(DefaultSs58Prefix::get(), 1)
		);
	})
}
