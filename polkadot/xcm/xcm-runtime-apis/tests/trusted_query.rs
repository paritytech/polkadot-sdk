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

use frame_support::sp_runtime::testing::H256;
use mock::*;
use sp_api::ProvideRuntimeApi;
use xcm::{prelude::*, v3};
use xcm_runtime_apis::trusted_query::{Error, TrustedQueryApi};

#[test]
fn query_trusted_reserve() {
	#[derive(Debug)]
	struct TestCase {
		name: &'static str,
		asset: VersionedAsset,
		location: VersionedLocation,
		expected: Result<bool, Error>,
	}

	sp_io::TestExternalities::default().execute_with(|| {
		let client = TestClient {};
		let runtime_api = client.runtime_api();

		let test_cases: Vec<TestCase> = vec![
			TestCase {
				// matches!(asset.id.0.unpack(), (1, [])) && matches!(origin.unpack(), (1,
				// [Parachain(1000)]))
				name: "Valid asset and location",
				asset: Asset { id: AssetId(Location::parent()), fun: Fungible(123) }.into(),
				location: (Parent, Parachain(1000)).into(),
				expected: Ok(true),
			},
			TestCase {
				name: "Invalid location and valid asset",
				asset: Asset { id: AssetId(Location::parent()), fun: Fungible(100) }.into(),
				location: (Parent, Parachain(1002)).into(),
				expected: Ok(false),
			},
			TestCase {
				name: "Valid location and invalid asset",
				asset: Asset { id: AssetId(Location::new(0, [])), fun: Fungible(100) }.into(),
				location: (Parent, Parachain(1000)).into(),
				expected: Ok(false),
			},
			TestCase {
				name: "Invalid asset conversion",
				asset: VersionedAsset::V3(v3::MultiAsset {
					id: v3::AssetId::Abstract([1; 32]),
					fun: v3::Fungibility::Fungible(1),
				}),
				location: (Parent, Parachain(1000)).into(),
				expected: Err(Error::VersionedAssetConversionFailed),
			},
		];

		for tc in test_cases {
			let res =
				runtime_api.is_trusted_reserve(H256::zero(), tc.asset.clone(), tc.location.clone());
			let inner_res = res.unwrap_or_else(|e| {
				panic!("Test case '{}' failed with ApiError: {:?}", tc.name, e);
			});

			assert_eq!(
				tc.expected, inner_res,
				"Test case '{}' failed: expected {:?}, got {:?}",
				tc.name, tc.expected, inner_res
			);
		}
	});
}

#[test]
fn query_trusted_teleporter() {
	#[derive(Debug)]
	struct TestCase {
		name: &'static str,
		asset: VersionedAsset,
		location: VersionedLocation,
		expected: Result<bool, Error>,
	}

	sp_io::TestExternalities::default().execute_with(|| {
		let client = TestClient {};
		let runtime_api = client.runtime_api();

		let test_cases: Vec<TestCase> = vec![
			TestCase {
				// matches!(asset.id.0.unpack(), (0, [])) && matches!(origin.unpack(), (1,
				// [Parachain(1000)]))
				name: "Valid asset and location",
				asset: Asset { id: AssetId(Location::new(0, [])), fun: Fungible(100) }.into(),
				location: (Parent, Parachain(1000)).into(),
				expected: Ok(true),
			},
			TestCase {
				name: "Invalid location and valid asset",
				asset: Asset { id: AssetId(Location::new(0, [])), fun: Fungible(100) }.into(),
				location: (Parent, Parachain(1002)).into(),
				expected: Ok(false),
			},
			TestCase {
				name: "Valid location and invalid asset",
				asset: Asset { id: AssetId(Location::new(1, [])), fun: Fungible(100) }.into(),
				location: (Parent, Parachain(1002)).into(),
				expected: Ok(false),
			},
			TestCase {
				name: "Invalid asset conversion",
				asset: VersionedAsset::V3(v3::MultiAsset {
					id: v3::AssetId::Abstract([1; 32]),
					fun: v3::Fungibility::Fungible(1),
				}),
				location: (Parent, Parachain(1000)).into(),
				expected: Err(Error::VersionedAssetConversionFailed),
			},
		];

		for tc in test_cases {
			let res = runtime_api.is_trusted_teleporter(
				H256::zero(),
				tc.asset.clone(),
				tc.location.clone(),
			);
			let inner_res = res.unwrap_or_else(|e| {
				panic!("Test case '{}' failed with ApiError: {:?}", tc.name, e);
			});

			assert_eq!(
				tc.expected, inner_res,
				"Test case '{}' failed: expected {:?}, got {:?}",
				tc.name, tc.expected, inner_res
			);
		}
	});
}
