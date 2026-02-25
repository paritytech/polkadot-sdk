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
